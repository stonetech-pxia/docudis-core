# 交接：检测请求加入与模型无关的“遮掩策略”（2026-10-02）

给下一个会话直接执行。写本文时 docudis-core HEAD 为 `740f5f7`。动手前先运行 `git status --short`，
不要 reset、覆盖或替用户提交无关改动；本任务在新分支上做，提交和推送前先问用户。

## 1. 背景与已经定下的事

1. Docudis 桌面版（Windows、macOS）要把 OpenAI Privacy Filter（OPF）作为第二个 NER 模型，和 XLM-R 一起运行。
   以后桌面版可能做成 agent：用户用大白话描述要遮什么，本地 LLM 把它转成结构化策略。
2. **模型差异全部留在 docudis-ner**（分词器、窗口、BIO / BIOES-Viterbi 解码、各模型档位）。这部分已在
   docudis-ner 完成（2026-10-02，见该仓库 `models/openai_privacy_filter/` 和 `models/README.md`）。
   **Core 不得出现任何按模型区分的代码**，依赖方向保持 `docudis-ner -> docudis-core`。
3. Core 已有的通用策略字段：`dictionary`（要遮的值）、`never_hide`（不遮的值）、`selection`
   （`RuleSelection`，只作用于正则规则）、`regions`。各模型结果通过 `detections` 进来。
4. **缺的两样，本任务要补**：按 `EntityType` 整类开关（对所有来源生效），以及只处理某些文本区间。

## 2. 为什么必须在 Core 里做，不能让宿主事后改 `enabled`

宿主现在可以在 `detect` 之后改 `enabled` 再调 `anonymize`，但这会漏：

- `pipeline.rs` 的 `resolve_inner`（约 211 行）只按 `source.priority()` 和长度决定重叠时谁留下，**不看 `enabled`**。
  一个随后会被关掉的片段，可能已经在重叠处理里挤掉了本该保留的片段。
- 例：模型把 `c/o Maria Lopez, 12 Calle Mayor` 整段标为 ADDRESS，里面的 PERSON 在 `resolve_overlaps`
  中被丢弃。用户说“地址不用遮”，宿主事后关掉 ADDRESS，人名随之暴露。
- 区间同理：`merge()`（约 236 行）会通过 `propagate()` 把同一个值传播到全文。事后按区间过滤，
  会漏掉跨边界的片段，也处理不好传播出去的副本。

因此过滤必须发生在 `DetectionPipeline::process` 内部、`resolve_overlaps` 之前。

## 3. 现状（`740f5f7`）

- `crates/docudis-core/src/pipeline.rs`
  - `DetectionPipeline::process`（约 60 行）：`without_title_lines` → `with_defaults` → 过滤
    `not_noise` / never-hide / `public_product` → `resolve_overlaps` → `repair_spans` → `merge`（含 `propagate`）。
  - `with_defaults`（约 80 行）：出生日期标签后的 DATE 改为 BIRTH_DATE；DATE、AMOUNT，以及表格行里来自
    模型或松散规则的 NUMBER，设为 `enabled = false`（检测到但默认不遮）。**这些默认值目前没有任何请求字段可以覆盖**。
- `crates/docudis-core/src/rules.rs`：`RuleSelection.categories` 按 `RuleCategory`（Contact、Temporal 等 13 类）
  过滤，只影响正则规则，不影响模型、词典、词表，而且分类和 `EntityType` 不一一对应。不要拿它实现本任务。
- `crates/docudis-capi/src/lib.rs`：`DetectRequest`（约 85 行）同时用于 `docudis_v1_detect_json` 和
  `docudis_v1_process_json`；`detect_request()`（约 351 行）组装候选后调用 `process`；
  `validate_detections()`（约 333 行）是现成的偏移校验写法。
- `bindings/dart/lib/src/core.dart`：`detect()`、`process()` 的参数和 `_detectRequest`。
- `crates/docudis-cli/src/main.rs`：CLI 参数解析。
- `conformance/fixtures/v1/pipeline.json`：现有 29 个流水线用例，Rust 和 Dart 测试共用。

## 4. 要实现的东西

### 4.1 请求格式（v1 内的纯增量字段）

```json
{
  "schema_version": 1,
  "text": "...",
  "policy": {
    "types": { "DATE": "hide", "ADDRESS": "keep", "URL": "off" },
    "ranges": [[1200, 5400], [8000, 9100]]
  }
}
```

- `policy` 可选，缺省或为 `null` 时行为必须与现在逐字节一致。`types`、`ranges` 也各自可选。
- `types` 的键是 `EntityType` 的序列化名（`PERSON`、`BIRTH_DATE`、`CUSTOM` ……）。未知名称返回
  `InvalidArgument`（不要静默忽略，否则用户以为关掉了其实没关）。值只有三种：
  - `hide`：该类片段 `enabled = true`，覆盖 `with_defaults` 的默认值（例如“日期也要遮”）。
  - `keep`：该类片段照常参与重叠处理，但 `enabled = false`（看得见、可在审阅界面手动打开）。
  - `off`：该类候选在 `resolve_overlaps` 之前直接丢弃，不参与重叠处理，也不会挤掉别的片段。
- `ranges`：UTF-8 字节偏移的半开区间，必须落在字符边界、`start < end`、不越界，否则 `InvalidArgument`。
  与任何区间都不相交的候选在 `resolve_overlaps` 之前丢弃。

### 4.2 在流水线中的位置

在 `process` 里，`with_defaults` 之后、`resolve_overlaps` 之前：

1. 丢弃 `off` 类型和区间外的候选；
2. 对剩余候选按 `hide` / `keep` 设置 `enabled`（`BIRTH_DATE` 的改名发生在 `with_defaults` 里，所以 `DATE` 和
   `BIRTH_DATE` 两个键各管各的，要有测试覆盖）；
3. `merge` 里 `propagate` 产生的新片段同样要过滤：落在区间外的丢弃，类型策略同样适用。

建议给 `DetectionPipeline` 加一个 `policy` 字段（或 `with_policy` 构造），不要改 `process` 的现有签名，
`DetectionPipeline::new` 的语义保持不变。

### 4.3 需要和用户确认的决策（先问，再写代码）

1. **词典命中（`DetectionSource::Dictionary`，类型 `CUSTOM`）是否受 `types` 和 `ranges` 约束？** 建议：受 `ranges`
   约束；不受 `types` 约束，因为用户亲手加的词典条目是更具体的意图。
2. **跨区间边界的候选**：建议“相交即保留”，宁可多遮半个词，也不要露出半个名字。
3. **传播**：区间内识别到的人名，在区间外的同名副本是否要遮？建议不遮，严格遵守“只处理这部分”。
4. `never_hide` 仍然优先于 `hide`（现有行为），保持不变。

### 4.4 兼容性

- 这是 v1 内的可选新字段，不改变任何现有行为，符合 README 的兼容约定，不需要新的符号命名空间。
- **风险**：旧版 Core 的 `DetectRequest` 没有 `deny_unknown_fields`，会静默忽略 `policy`。宿主必须依赖版本固定，
  Android 已按 commit 固定 Core。在 `crates/README.md` 的请求说明里写明“需要 Core ≥ 本次提交”，并让
  `docudis_v1_version()` 的版本号随之递增，宿主可以据此检查。
- conformance：**不要改 `pipeline.json` 的结构**。新增 `conformance/fixtures/v1/policy.json`（新文件，用例带
  `policy` 输入），Rust 和 Dart 测试都消费它，并在 `conformance/README.md` 里登记。

### 4.5 要改的地方

- `crates/docudis-core/src/pipeline.rs`：策略类型、过滤逻辑、单元测试。
- `crates/docudis-capi/src/lib.rs`：`DetectRequest.policy`、校验、测试（未知类型、坏区间、缺省等价）。
- `crates/docudis-capi/include/docudis.h` 里请求格式的注释、`crates/README.md`、`README.md`。
- `crates/docudis-cli/src/main.rs`：`--type DATE=hide`（可重复）、`--range START:END`（可重复）。
- `bindings/dart/lib/src/core.dart`（和 `models.dart`）：`detect()` / `process()` 增加可选 `policy` 参数。
  Dart 侧是 UTF-16 偏移，区间要和现有 detections 一样经 `offsets.dart` 转换。
- `conformance/fixtures/v1/policy.json` 及 Rust、Dart 两侧的测试。

## 5. 验收

至少覆盖这些用例（Rust 单元测试和 conformance 各一份）：

1. 不带 `policy`：现有全部测试和 conformance 不变。
2. `{"DATE": "hide"}`：日期被遮；出生日期仍为 `BIRTH_DATE`，按它自己的规则处理。
3. `{"ADDRESS": "off"}` + 模型把 `c/o Maria Lopez, 12 Calle Mayor` 标为 ADDRESS、内部另有 PERSON：PERSON 必须保留并被遮。
4. `{"ADDRESS": "keep"}`：同一文本中，ADDRESS 片段存在但 `enabled = false`。
5. `ranges` 只含第二段：第一段里的同名人名不被遮；跨边界的人名整体保留并被遮（若 4.3 第 2 条按建议定）。
6. 未知类型名、非字符边界区间、越界区间：`InvalidArgument`，错误信息可读。
7. README 里的整套本地检查（fmt、clippy、test、release build、`test-c-header.sh`、Dart format/analyze/test）全部通过。

## 6. 不要做的事

- 不要加任何和具体模型（XLM-R、OPF）相关的代码或配置。
- 不要改 `data/rules`，不要改现有 conformance 文件的结构。
- 不要顺手改 `with_defaults` 的默认值：DATE、AMOUNT 默认不遮是现有产品口径，只允许通过 `policy` 覆盖。
- 不要实现 agent、LLM 或自然语言解析，那属于桌面应用层。

## 7. 相关资料

- docudis-ner：`models/openai_privacy_filter/model.json`、`models/README.md`（OPF 的标签口径、内存与窗口限制）。
- docudis-android：`docs/HANDOFF-no-company-mobile-ner-2026-09-30.md`（默认不自动遮公司名，OPF 不覆盖公司名与此一致）。
- 2026-10-01 在 docudis-android 测试集上的对比（新口径、完整管线）：OPF 单独泄漏率 6–27%，XLM-R 为 0.5–7%；
  两者并集把 consumer 从 4.8% 降到 3.7%、regression 从 7.3% 降到 5.3%，代价是误遮多 30–40%、
  速度慢约 5 倍。OPF 的价值在于和 XLM-R 一起作为候选来源，不在于替代 XLM-R。

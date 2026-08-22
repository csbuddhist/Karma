# Browser URL policy and multilingual title-keyword research / 浏览器网址策略与多语种窗口标题关键词调研

Research date / 调研日期：2026-08-23

This note evaluates source material for browser allow/deny rules and a high-precision multilingual adult-content title signal. It does **not** approve an upstream list for unconditional window closure and intentionally does not reproduce sensitive terms. / 本文评估浏览器白名单/黑名单规则，以及用于窗口标题判定的高精度多语种成人内容信号。本文**不**批准直接使用任何上游整表来无条件关窗，也有意不复录敏感词本身。

## Executive recommendation / 结论先行

1. Use language-specific classified sources where possible: the MIT-licensed [`sexual`-tagged English entries](https://github.com/dsojevic/profanity-list/blob/c27924319aa9bd6f917e3782b4f4b6604a50b652/en.json) and the MIT-licensed Japanese [`Sexual.txt`](https://github.com/MosasoM/inappropriate-words-ja/blob/0f8ad0eec9794b1633c1a0e426ae73ea3aff0073/Sexual.txt) are better starting points than a generic profanity list. Both are stale and still require native-speaker review and negative tests. / 尽量采用按语言分类的来源：MIT 许可、带 [`sexual` 标签的英语词条](https://github.com/dsojevic/profanity-list/blob/c27924319aa9bd6f917e3782b4f4b6604a50b652/en.json)，以及 MIT 许可的日语 [`Sexual.txt`](https://github.com/MosasoM/inappropriate-words-ja/blob/0f8ad0eec9794b1633c1a0e426ae73ea3aff0073/Sexual.txt)，都比通用脏词表更适合作为起点。两者均较久未维护，仍需母语者审核和反例测试。
2. For Chinese and Russian there is no located source that is simultaneously adult-only, current, permissively licensed, and provenance-clean. The Chinese [sensitive-stop-words adult category](https://github.com/fwwdn/sensitive-stop-words/blob/a7d06bb1c321e669943b6841570d9da6dad8ce2b/%E8%89%B2%E6%83%85%E7%B1%BB.txt) is narrow and repository-labeled Apache-2.0 but lacks item-level provenance. Meta's [Toxicity-200](https://github.com/facebookresearch/flores/blob/a6c830c6e1051fb4ac1a44b32358f00463f332bd/toxicity/README.md) distinguishes Simplified Chinese, Traditional Chinese, and Russian and was collected by human translation, but mixes pornographic terms with insults, hate speech, and anatomy under CC BY-SA 4.0. Use both only as reviewed candidate sources. / 中文和俄语尚未找到同时满足“仅成人内容、仍在维护、宽松许可、来源链清晰”的来源。中文 [sensitive-stop-words 色情分类](https://github.com/fwwdn/sensitive-stop-words/blob/a7d06bb1c321e669943b6841570d9da6dad8ce2b/%E8%89%B2%E6%83%85%E7%B1%BB.txt) 分类较窄、仓库标注 Apache-2.0，但缺少逐词来源。Meta [Toxicity-200](https://github.com/facebookresearch/flores/blob/a6c830c6e1051fb4ac1a44b32358f00463f332bd/toxicity/README.md) 区分简体中文、繁体中文和俄语，并通过人工翻译收集，但在 CC BY-SA 4.0 下混合了色情词、侮辱、仇恨言论和人体部位。两者都只能作为经审核的候选来源。
3. Use the [LDNOOBW lists](https://github.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words/tree/5faf2ba42d7b1c0977169ec3611df25a3c08eb13) only as a **cross-language seed for human curation**. They cover English, Chinese, Japanese, and Russian under CC BY 4.0, but are broad profanity lists, have no adult-only category, and explicitly describe their contents as subjective. / 仅将 [LDNOOBW 词表](https://github.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words/tree/5faf2ba42d7b1c0977169ec3611df25a3c08eb13) 作为**跨语言人工筛选种子**。它以 CC BY 4.0 覆盖英语、中文、日语和俄语，但属于宽泛脏词表、没有“仅色情”分类，而且上游明确说明选词具有主观性。
4. Use [HurtLex](https://github.com/valeriobasile/hurtlex/tree/d4d5cf1199c09868486f978fcea58af0e8936a1e) only for research and comparison unless distribution is confirmed compatible with CC BY-NC-SA 4.0. Its license forbids commercial use and requires ShareAlike; its README also acknowledges automatically introduced language errors. / 除非分发方式经确认兼容 CC BY-NC-SA 4.0，否则 [HurtLex](https://github.com/valeriobasile/hurtlex/tree/d4d5cf1199c09868486f978fcea58af0e8936a1e) 只能用于研究和对照。其许可证禁止商业使用并要求相同方式共享，README 也承认自动处理引入了语言错误。
5. Create a small Karma-owned, reviewed data pack with provenance per term, negative tests, match mode, language/script, and a `direct_close` classification. A bare anatomy, health, education, news, or identity term must never be enough for direct closure. Sexual orientation and gender-identity vocabulary must be excluded from the adult-content trigger set. / 建立一个由 Karma 自行维护、小规模、经过审核的数据包；每个词条记录来源、反例测试、匹配方式、语言/文字系统和 `direct_close` 分类。单独的人体部位、健康、教育、新闻或身份词绝不能直接触发关窗。性取向和性别认同词必须排除在成人内容触发集之外。
6. Enforce browser policy in this order: **allowlisted host → allow; denylisted host → close; high-precision title match → close; image score at/above threshold → close; otherwise no close**. Reject conflicting configuration or explicitly make allowlist precedence win because “allowlisted sites are never closed” is the requested invariant. / 浏览器策略固定为：**白名单主机 → 放行；黑名单主机 → 关闭；高精度标题命中 → 关闭；图像分数达到阈值 → 关闭；否则不关闭**。应拒绝冲突配置，或明确白名单优先，因为“白名单网站永不关闭”是需求中的不变量。

## Candidate lexicons / 候选词库

### 1. dsojevic/profanity-list — classified English seed / 带分类的英语种子

The pinned [`en.json`](https://github.com/dsojevic/profanity-list/blob/c27924319aa9bd6f917e3782b4f4b6604a50b652/en.json) has 434 entries, of which 262 carry the `sexual` tag. Its structured records include severity, match variants, partial-match guidance, and some exception patterns. This is materially safer to curate than an untagged text file. / 固定版本 [`en.json`](https://github.com/dsojevic/profanity-list/blob/c27924319aa9bd6f917e3782b4f4b6604a50b652/en.json) 共 434 条，其中 262 条带 `sexual` 标签。结构化记录包含严重度、匹配变体、部分匹配指引和部分例外模式，比无标签纯文本更适合安全筛选。

The [README](https://github.com/dsojevic/profanity-list/tree/c27924319aa9bd6f917e3782b4f4b6604a50b652) warns that most severity values began at an unvalidated default and that profanity judgments are subjective. The repository has only one commit, dated 2021-10-23, so maintenance freshness is poor. Its separate `lgbtq` tag must not be imported into the adult-content policy, and overlap between tags must be explicitly removed during curation. / [README](https://github.com/dsojevic/profanity-list/tree/c27924319aa9bd6f917e3782b4f4b6604a50b652) 警告大多数严重度最初只是未经验证的默认值，且脏词判断具有主观性。仓库只有一个提交，日期为 2021-10-23，维护新鲜度较差。独立的 `lgbtq` 标签不得导入成人内容策略；筛选时也必须显式移除标签交集中的身份词。

License / 许可证：[MIT](https://github.com/dsojevic/profanity-list/blob/c27924319aa9bd6f917e3782b4f4b6604a50b652/LICENSE), requiring preservation of the copyright and permission notice in copies or substantial portions. / [MIT](https://github.com/dsojevic/profanity-list/blob/c27924319aa9bd6f917e3782b4f4b6604a50b652/LICENSE)，复制或分发实质部分时需保留版权与许可声明。

Verdict / 结论：best located English starting point, but import only a manually reviewed subset of `sexual`, not the tag wholesale. / 当前找到的最佳英语起点，但只能导入人工审核后的 `sexual` 子集，不能整标签导入。

### 2. MosasoM/inappropriate-words-ja — classified Japanese seed / 带分类的日语种子

The pinned [`Sexual.txt`](https://github.com/MosasoM/inappropriate-words-ja/blob/0f8ad0eec9794b1633c1a0e426ae73ea3aff0073/Sexual.txt) contains 281 non-empty entries. The [README](https://github.com/MosasoM/inappropriate-words-ja/tree/0f8ad0eec9794b1633c1a0e426ae73ea3aff0073) says the list was manually collected using the criterion that the word itself can be judged inappropriate; it gives an example of deliberately excluding an ambiguous term with a benign body-care meaning. That principle aligns with a high-precision direct-close tier. / 固定版本 [`Sexual.txt`](https://github.com/MosasoM/inappropriate-words-ja/blob/0f8ad0eec9794b1633c1a0e426ae73ea3aff0073/Sexual.txt) 含 281 个非空词条。[README](https://github.com/MosasoM/inappropriate-words-ja/tree/0f8ad0eec9794b1633c1a0e426ae73ea3aff0073) 称词表由人工收集，标准是“仅凭词本身即可判断为不适当”；还举例说明刻意排除了具有正常身体护理含义的歧义词。这一原则与高精度直接关窗层较一致。

The repository also provides 2,630 mechanically generated masked variants and 150 lookalike-replacement variants, but its author explicitly calls the masked-generation accuracy poor. Keep generated variants out of direct-close rules until each variant has negative tests. The pinned main data commit is from 2021-12-01. / 仓库还提供 2,630 个机械生成的遮罩变体和 150 个形似替换变体，但作者明确认为遮罩生成精度不佳。在每个变体完成反例测试前，不得进入直接关窗规则。固定主数据提交日期为 2021-12-01。

License / 许可证：[MIT](https://github.com/MosasoM/inappropriate-words-ja/blob/0f8ad0eec9794b1633c1a0e426ae73ea3aff0073/LICENSE). / [MIT](https://github.com/MosasoM/inappropriate-words-ja/blob/0f8ad0eec9794b1633c1a0e426ae73ea3aff0073/LICENSE)。

Verdict / 结论：best located Japanese starting point; review the base file first and treat generated obfuscations as a later, lower-confidence phase. / 当前找到的最佳日语起点；先审核基础文件，机械生成的规避写法只作为后续低置信阶段。

### 3. LDNOOBW — cross-language permissive seed / 跨语言宽松许可种子

The upstream [README](https://github.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words/blob/5faf2ba42d7b1c0977169ec3611df25a3c08eb13/README.md) says Shutterstock used the lists to keep undesirable suggestions out of autocomplete and recommendation results. It also says inclusion is subjective and varies by culture, language, and geography. That purpose is materially broader than detecting pornographic browser titles. / 上游 [README](https://github.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words/blob/5faf2ba42d7b1c0977169ec3611df25a3c08eb13/README.md) 说明 Shutterstock 用这些词表过滤自动补全与推荐结果中的不适当建议，同时明确指出收录标准具有主观性，并随文化、语言和地域变化。这一用途明显宽于“识别色情浏览器标题”。

Pinned source files / 固定版本源文件：

- [English / 英语](https://github.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words/blob/5faf2ba42d7b1c0977169ec3611df25a3c08eb13/en) — 403 non-empty lines / 403 个非空行。
- [Chinese / 中文](https://github.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words/blob/5faf2ba42d7b1c0977169ec3611df25a3c08eb13/zh) — 319 non-empty lines / 319 个非空行。
- [Japanese / 日语](https://github.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words/blob/5faf2ba42d7b1c0977169ec3611df25a3c08eb13/ja) — 180 non-empty lines / 180 个非空行。
- [Russian / 俄语](https://github.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words/blob/5faf2ba42d7b1c0977169ec3611df25a3c08eb13/ru) — 151 non-empty lines / 151 个非空行。

The counts above are reproducible counts of non-empty lines at the pinned commit, not a claim that every line is unique, correctly spelled, adult-related, or safe to use. The upstream exposes one `zh` file and makes no stated Simplified/Traditional coverage guarantee. / 上述数字只是固定提交中非空行的可复现计数，并不表示每行都唯一、拼写正确、属于成人内容或适合使用。上游仅提供一个 `zh` 文件，也没有承诺简体/繁体覆盖程度。

License / 许可证：[repository license](https://github.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words/blob/5faf2ba42d7b1c0977169ec3611df25a3c08eb13/LICENSE), CC BY 4.0. The official [CC BY 4.0 deed](https://creativecommons.org/licenses/by/4.0/) permits commercial sharing and adaptation but requires attribution, a license link, and an indication of modifications. A curated derivative therefore needs packaged attribution and a clear modification notice. / [仓库许可证](https://github.com/LDNOOBW/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words/blob/5faf2ba42d7b1c0977169ec3611df25a3c08eb13/LICENSE) 为 CC BY 4.0。官方 [CC BY 4.0 说明](https://creativecommons.org/licenses/by/4.0/) 允许商业分享与改编，但要求署名、许可证链接并说明修改。由此筛选得到的衍生词表需要随包提供署名和明确的修改说明。

Verdict / 结论：legally plausible as a seed with attribution; technically unsafe as an unreviewed direct-close list. / 在正确署名的前提下可作为种子使用；未经审核时，技术上不适合作为直接关窗词表。

### 4. HurtLex — useful taxonomy, incompatible default license / 分类有用，但默认许可证不兼容

The [HurtLex README](https://github.com/valeriobasile/hurtlex/blob/d4d5cf1199c09868486f978fcea58af0e8936a1e/README.md) provides version 1.2 lexica for [English](https://github.com/valeriobasile/hurtlex/blob/d4d5cf1199c09868486f978fcea58af0e8936a1e/lexica/EN/1.2/hurtlex_EN.tsv), [Chinese](https://github.com/valeriobasile/hurtlex/blob/d4d5cf1199c09868486f978fcea58af0e8936a1e/lexica/ZH/1.2/hurtlex_ZH.tsv), [Japanese](https://github.com/valeriobasile/hurtlex/blob/d4d5cf1199c09868486f978fcea58af0e8936a1e/lexica/JA/1.2/hurtlex_JA.tsv), and [Russian](https://github.com/valeriobasile/hurtlex/blob/d4d5cf1199c09868486f978fcea58af0e8936a1e/lexica/RU/1.2/hurtlex_RU.tsv). It annotates categories such as male anatomy (`ASM`), female anatomy (`ASF`), and prostitution (`PR`), allowing narrower candidate extraction than LDNOOBW. / [HurtLex README](https://github.com/valeriobasile/hurtlex/blob/d4d5cf1199c09868486f978fcea58af0e8936a1e/README.md) 提供 1.2 版[英语](https://github.com/valeriobasile/hurtlex/blob/d4d5cf1199c09868486f978fcea58af0e8936a1e/lexica/EN/1.2/hurtlex_EN.tsv)、[中文](https://github.com/valeriobasile/hurtlex/blob/d4d5cf1199c09868486f978fcea58af0e8936a1e/lexica/ZH/1.2/hurtlex_ZH.tsv)、[日语](https://github.com/valeriobasile/hurtlex/blob/d4d5cf1199c09868486f978fcea58af0e8936a1e/lexica/JA/1.2/hurtlex_JA.tsv)和[俄语](https://github.com/valeriobasile/hurtlex/blob/d4d5cf1199c09868486f978fcea58af0e8936a1e/lexica/RU/1.2/hurtlex_RU.tsv)词表。它标注了男性人体部位（`ASM`）、女性人体部位（`ASF`）和性交易（`PR`）等类别，比 LDNOOBW 更便于窄化候选。

However, the project describes 1.x entries as automatically processed translations, asks contributors to correct misspellings, wrong lemmas, and inflected forms, and includes a historical homosexuality-related category. That category is not a proxy for adult content and must not be imported. Anatomy and prostitution categories also contain legitimate medical, legal, academic, and news usage. / 但项目将 1.x 词条描述为自动处理的翻译，并请贡献者修正拼写、词元和屈折形式错误；它还含有历史形成的同性恋相关类别。该类别不能代表成人内容，必须禁止导入。人体部位和性交易类别同样包含合法的医学、法律、学术和新闻语境。

License / 许可证：the README embeds CC BY-NC-SA 4.0; the official [license deed](https://creativecommons.org/licenses/by-nc-sa/4.0/) prohibits commercial use and requires adaptations to be shared under the same license. / README 内嵌 CC BY-NC-SA 4.0；官方[许可证说明](https://creativecommons.org/licenses/by-nc-sa/4.0/) 禁止商业使用，并要求改编作品使用相同许可证。

Verdict / 结论：use for offline comparison, gap analysis, and evaluation only; do not vendor into a normally distributed Karma build without a documented license decision. / 仅用于离线对照、缺口分析和评估；在没有书面许可证决策前，不得随常规 Karma 构建分发。

### 5. sensitive-stop-words — narrow Chinese discovery source with provenance risk / 分类较窄但来源链有风险的中文发现源

The repository [README](https://github.com/fwwdn/sensitive-stop-words/blob/a7d06bb1c321e669943b6841570d9da6dad8ce2b/README.md) describes an adult-content category of roughly 300 comma-separated entries; the pinned file contains 304 non-empty comma-separated entries. It labels the project Apache-2.0 and provides a [license file](https://github.com/fwwdn/sensitive-stop-words/blob/a7d06bb1c321e669943b6841570d9da6dad8ce2b/LICENSE). / 仓库 [README](https://github.com/fwwdn/sensitive-stop-words/blob/a7d06bb1c321e669943b6841570d9da6dad8ce2b/README.md) 将色情分类描述为约 300 个逗号分隔词条；固定版本文件实际有 304 个非空逗号分隔词条。仓库标注 Apache-2.0，并提供[许可证文件](https://github.com/fwwdn/sensitive-stop-words/blob/a7d06bb1c321e669943b6841570d9da6dad8ce2b/LICENSE)。

The same README says the material was assembled from public Internet information and asks rights holders to request removal, but does not establish item-level ownership or upstream licenses. A repository license cannot by itself cure missing rights in third-party data. The official [Apache 2.0 text](https://www.apache.org/licenses/LICENSE-2.0) also requires preservation of the license, modification notices, applicable attribution notices, and any upstream `NOTICE`. / 同一 README 又称材料整理自互联网公开信息，并请权利人联系删除，却没有证明逐词权属或上游许可证。仓库许可证本身不能补足第三方数据可能缺失的授权。官方 [Apache 2.0 文本](https://www.apache.org/licenses/LICENSE-2.0) 还要求保留许可证、修改说明、适用的署名，以及上游已有的 `NOTICE`。

The source does not document separate Simplified, Taiwan Traditional, or Hong Kong Traditional coverage. [OpenCC](https://github.com/BYVoid/OpenCC/tree/4f90418b9ed73a91023897095c762e5fdaadc016), Apache-2.0, provides `s2t`, `s2tw`, `s2hk`, and reverse conversion configurations, but generated variants still require native-speaker review because script conversion does not establish regional meaning or actual usage. / 该来源没有分别说明简体、台湾正体和香港繁体覆盖率。[OpenCC](https://github.com/BYVoid/OpenCC/tree/4f90418b9ed73a91023897095c762e5fdaadc016) 采用 Apache-2.0，提供 `s2t`、`s2tw`、`s2hk` 及反向转换配置；但转换生成的变体仍需母语者审核，因为字形转换不能证明区域语义和实际用法。

Verdict / 结论：good for finding candidate concepts and regional variants; do not copy wholesale until provenance is auditable. / 适合发现候选概念和地区变体；来源链可审核之前不得整体复制。

### 6. Meta Toxicity-200 — script-aware cross-check, not an adult-only list / 可区分文字系统的交叉校验源，并非色情专用表

Meta's pinned [Toxicity-200 documentation](https://github.com/facebookresearch/flores/blob/a6c830c6e1051fb4ac1a44b32358f00463f332bd/toxicity/README.md) says the lists were collected through human translation for translation-safety research. It explicitly includes pornographic terms, but also profanity, insults, hate speech, bullying language, and body-part terminology. It therefore cannot be used as an adult-content list without item-by-item classification. / Meta 固定版本 [Toxicity-200 文档](https://github.com/facebookresearch/flores/blob/a6c830c6e1051fb4ac1a44b32358f00463f332bd/toxicity/README.md) 称词表通过人工翻译收集，用于翻译安全研究。它明确包含色情词，但也同时包含脏话、侮辱、仇恨言论、霸凌语言和人体部位术语，因此未经逐词分类不能作为成人内容词表使用。

The language catalog separately identifies `eng_Latn`, `jpn_Jpan`, `rus_Cyrl`, `zho_Hans`, and `zho_Hant`, making it a useful cross-check for the exact scripts requested. The same documentation warns that translation access and annotator backgrounds introduce bias. The repository states it is no longer updated and was archived in 2023. / 语言目录分别标识 `eng_Latn`、`jpn_Jpan`、`rus_Cyrl`、`zho_Hans` 和 `zho_Hant`，适合交叉检查需求指定的文字系统。文档同时警告译者资源与标注者背景会引入偏差。仓库声明不再更新，并于 2023 年归档。

License / 许可证：the repository's [license table](https://github.com/facebookresearch/flores/blob/a6c830c6e1051fb4ac1a44b32358f00463f332bd/README.md#licenses) assigns Toxicity-200 CC BY-SA 4.0. Commercial use is not prohibited, but attribution and ShareAlike obligations require a deliberate packaging decision for any derivative data. / 仓库[许可证表](https://github.com/facebookresearch/flores/blob/a6c830c6e1051fb4ac1a44b32358f00463f332bd/README.md#licenses) 将 Toxicity-200 标为 CC BY-SA 4.0。它不禁止商业使用，但署名与相同方式共享义务意味着任何衍生数据都需要明确的打包许可决策。

Verdict / 结论：use for Simplified/Traditional coverage checks and Russian candidate review; do not treat occurrence in this mixed toxicity list as sufficient evidence for direct closure. / 用于简繁覆盖校验和俄语候选审核；词条出现在这个混合有毒内容词表中，不足以证明可以直接关窗。

### 7. LDNOOBW V2 — broad coverage with unresolved upstream licensing / 覆盖广但上游许可证链未解决

The pinned [LDNOOBW V2 README](https://github.com/LDNOOBWV2/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words_V2/tree/e2f7430cde6fcc755eca7243d5cf46fc0766ff29) describes the project as an aggregation and extension of the original Shutterstock list plus many web profanity lists. It reports 12,996 English, 1,811 Chinese, 468 Japanese, and 4,948 Russian entries, but also says native-speaker review is needed and recommends hard-coded lists only as an additional quality criterion or ML input. The data mixes profanity, hate, medical/anatomical, identity, and explicit content rather than providing an adult-only class. / 固定版本 [LDNOOBW V2 README](https://github.com/LDNOOBWV2/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words_V2/tree/e2f7430cde6fcc755eca7243d5cf46fc0766ff29) 将项目描述为原 Shutterstock 词表与大量网络脏词表的聚合扩展。它报告英语 12,996、中文 1,811、日语 468、俄语 4,948 条，但同时说明需要母语者审核，并建议硬编码词表只作为辅助质量指标或机器学习输入。数据混合了脏话、仇恨、医学/人体、身份和明确成人内容，没有色情专用分类。

The repository places its aggregate under [CC0-1.0](https://github.com/LDNOOBWV2/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words_V2/blob/e2f7430cde6fcc755eca7243d5cf46fc0766ff29/LICENSE), but its [SOURCES.md](https://github.com/LDNOOBWV2/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words_V2/blob/e2f7430cde6fcc755eca7243d5cf46fc0766ff29/SOURCES.md) lists dozens of upstream repositories without recording an item-to-source map or each source's license. It explicitly begins with the original CC BY 4.0 Shutterstock list and also lists sources with their own terms. A downstream aggregator cannot unilaterally waive rights or attribution requirements it does not own. The CC0 label therefore does not establish that every aggregated term is safely CC0. / 仓库将聚合物标为 [CC0-1.0](https://github.com/LDNOOBWV2/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words_V2/blob/e2f7430cde6fcc755eca7243d5cf46fc0766ff29/LICENSE)，但其 [SOURCES.md](https://github.com/LDNOOBWV2/List-of-Dirty-Naughty-Obscene-and-Otherwise-Bad-Words_V2/blob/e2f7430cde6fcc755eca7243d5cf46fc0766ff29/SOURCES.md) 罗列数十个上游仓库，却没有逐词来源映射或逐源许可证记录。它明确以原 CC BY 4.0 Shutterstock 词表为起点，也列出带各自条款的其他来源。下游聚合者不能单方面放弃其不拥有的权利或署名义务，因此 CC0 标签不能证明每个聚合词条都可安全视为 CC0。

Verdict / 结论：use only to discover coverage gaps during research; do not copy or redistribute entries from V2 unless each retained entry is traced to a compatible upstream source. Never import the full language files into a match-means-close policy. / 仅用于研究阶段发现覆盖缺口；除非每个保留词条都能追溯到兼容的上游来源，否则不得从 V2 复制或再分发。绝不能把完整语言文件导入“命中即关闭”策略。

## Required curation model / 必需的词表治理模型

A safe product list should be a versioned data asset, not an anonymous newline file. Each entry should contain at least: / 安全的产品词表应是有版本的数据资产，而不是匿名的逐行文本。每个词条至少应包含：

- stable term ID, locale, script, normalized form, and original reviewed form / 稳定词条 ID、语言区域、文字系统、规范化形式和经审核的原始形式；
- match mode: `whole_token`, `phrase`, or reviewed `cjk_substring`; never an implicit substring default / 匹配方式：`whole_token`、`phrase` 或经审核的 `cjk_substring`；不得默认隐式子串匹配；
- action class: `direct_close` or `supporting_signal`; the direct-close class should be deliberately small / 动作分类：`direct_close` 或 `supporting_signal`；直接关窗类应刻意保持很小；
- exact source URL and pinned commit, license, reviewer, review date, and modification note / 精确源链接、固定提交、许可证、审核人、审核日期和修改说明；
- positive tests plus negative-context and in-word collision tests / 正例测试、负面语境反例测试和词内碰撞测试；
- optional exception phrases, but no exception may override an explicit browser allowlist because the allowlist already resolves the action / 可选例外短语；但词条例外不应覆盖显式浏览器白名单，因为白名单已经决定动作。

Do not direct-close on a single character, a very short ambiguous token, anatomy alone, reproductive or sexual-health terminology, educational/legal/news discussion, relationship vocabulary, or identity/orientation vocabulary. Phrase-level evidence and manually validated commercial/adult-media context are substantially safer. / 不得因单字、极短歧义词、单独的人体部位、生殖或性健康术语、教育/法律/新闻讨论、关系词或身份/性取向词直接关窗。短语级证据，以及人工验证过的商业成人媒体语境，安全性明显更高。

Lexicons evolve and are culturally situated. Add a release process with native-speaker review for each language/region, a false-positive regression corpus, a reversible deprecation list, and a data-pack version in audit events. Never log the complete foreground title or matched raw text; log a term ID, language, rule version, and disposition reason. / 词表会变化，也受文化语境影响。发布流程应包括各语言/地区母语者审核、误报回归语料、可撤销的弃用列表，以及审计事件中的数据包版本。不得记录完整前台标题或命中的原文；只记录词条 ID、语言、规则版本和处置原因。

## Title acquisition and matching / 标题获取与匹配

Microsoft documents that [`GetForegroundWindow`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getforegroundwindow) returns the window currently being used, but may return `NULL` while activation changes. [`GetWindowTextW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowtextw) retrieves a cross-process window caption, not arbitrary control text; it can truncate to the provided buffer. [`GetWindowTextLengthW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowtextlengthw) can overestimate length but is safe for sizing. / Microsoft 文档说明，[`GetForegroundWindow`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getforegroundwindow) 返回用户当前操作窗口，但在激活切换时可能返回 `NULL`。[`GetWindowTextW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowtextw) 可读取跨进程窗口标题，但不能读取任意控件文本，并可能按缓冲区截断。[`GetWindowTextLengthW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getwindowtextlengthw) 可能高估长度，但可用于分配缓冲区。

Recommended snapshot / 建议快照流程：

1. Capture foreground `HWND`, PID, process creation identity, and title together; a `NULL` handle or failed/empty caption means “no title signal,” not a match. / 同时获取前台 `HWND`、PID、进程创建身份和标题；句柄为 `NULL` 或标题读取失败/为空，只表示“无标题信号”，不能算命中。
2. Use only UTF-16 `W` APIs, cap the title buffer (for example 4096 UTF-16 code units), strip NUL/control characters, and discard the raw title after decision. / 只使用 UTF-16 `W` API，限制标题缓冲区（例如 4096 个 UTF-16 码元），移除 NUL/控制字符，并在决策后丢弃原始标题。
3. Revalidate the same process identity before disposition. A match against one foreground title must never terminate a later process that reused the PID or a newly focused window. / 处置前重新验证同一进程身份。对某个前台标题的命中，绝不能终止后来复用 PID 的进程或新获得焦点的窗口。

Recommended text pipeline / 建议文本管线：

1. Apply the same Unicode normalization to dictionary and title. [UAX #15](https://www.unicode.org/reports/tr15/) defines canonical normalization; its [normalization FAQ](https://www.unicode.org/faq/normalization.html) says NFKC is useful for loose matching but loses compatibility distinctions. NFKC is useful here for width/compatibility variants only if its new false positives are tested. Karma already uses the Rust [`unicode-normalization`](https://docs.rs/unicode-normalization/latest/unicode_normalization/) implementation elsewhere. / 对词典和标题采用完全相同的 Unicode 规范化。[UAX #15](https://www.unicode.org/reports/tr15/) 定义规范化；其[规范化 FAQ](https://www.unicode.org/faq/normalization.html) 指出 NFKC 适合宽松匹配，但会丢失兼容性差异。这里可用 NFKC 处理宽度/兼容变体，但必须测试其新增误报。Karma 其他模块已使用 Rust [`unicode-normalization`](https://docs.rs/unicode-normalization/latest/unicode_normalization/) 实现。
2. Use Unicode case folding for cased scripts, not ASCII-only lowercasing. Keep original text out of logs. / 对有大小写的文字系统使用 Unicode case folding，而不是仅 ASCII 小写转换；原始文本不得进入日志。
3. For English and Russian, use phrase and whole-token matching over Unicode word boundaries; account for Russian inflection through reviewed surface forms or morphology, not unchecked fuzzy matching. / 英语和俄语使用基于 Unicode 词边界的短语与整词匹配；俄语屈折变化通过审核过的词形或形态分析处理，不使用未经约束的模糊匹配。
4. Do not assume generic tokenization works for Chinese or Japanese. [UAX #29](https://www.unicode.org/reports/tr29/) explicitly says reliable Chinese and Japanese word boundaries require dictionary lookup or other tailored mechanisms. Use a locale-aware segmenter or a reviewed multi-pattern matcher over normalized CJK text, with explicit per-term substring rules and no one-character direct-close entries. / 不要假设通用分词适用于中文和日语。[UAX #29](https://www.unicode.org/reports/tr29/) 明确指出，可靠的中文和日语词边界需要词典查找或其他定制机制。应使用语言区域感知的分词器，或在规范化 CJK 文本上使用经审核的多模式匹配器；每个子串规则必须显式声明，且禁止单字直接关窗。
5. [UTS #39](https://unicode.org/reports/tr39/) describes Unicode confusables, including Latin/Cyrillic lookalikes, but says confusability is not exact science. Treat a confusable skeleton as an optional supporting signal, never as automatic expansion of the direct-close list. / [UTS #39](https://unicode.org/reports/tr39/) 描述了 Unicode 易混淆字符，包括拉丁/西里尔形似字符，但也指出混淆性并非精确科学。混淆骨架只能作为可选辅助信号，绝不能自动扩展直接关窗词表。

## URL parsing and host matching / URL 解析与主机匹配

Never compare an allow/deny rule with the raw URL string, browser title, address-bar display text, or a substring of any of them. Parse a trustworthy current-tab URL and compare the structured host. A window title is not proof of the active URL. If the browser integration cannot obtain a trustworthy URL, report “unknown URL” and continue only with title/image policy; never infer allowlist membership. / 不得把白名单/黑名单规则与原始 URL 字符串、浏览器标题、地址栏显示文本或其任意子串比较。必须获取可信的当前标签页 URL，解析后比较结构化主机。窗口标题不能证明当前 URL。若浏览器集成无法获得可信 URL，应报告“URL 未知”，只继续标题/图像策略；绝不能推断白名单命中。

The [WHATWG URL Standard](https://url.spec.whatwg.org/) defines browser URL parsing and host serialization. Its examples show domain case folding, IDN conversion to ASCII/Punycode, unusual IPv4 canonicalization, structured IPv6, and that `example.com` and `example.com.` are normally distinct. Rust's [`url` crate](https://docs.rs/url/latest/url/) implements that standard; [`Host`](https://docs.rs/url/latest/url/enum.Host.html) distinguishes domains, IPv4, and IPv6 and documents their serialized forms. / [WHATWG URL 标准](https://url.spec.whatwg.org/)定义浏览器 URL 的解析与主机序列化；示例涵盖域名大小写归一、IDN 转 ASCII/Punycode、特殊 IPv4 规范化、结构化 IPv6，并指出 `example.com` 与 `example.com.` 通常不同。Rust [`url` crate](https://docs.rs/url/latest/url/) 实现该标准；[`Host`](https://docs.rs/url/latest/url/enum.Host.html) 区分域名、IPv4 和 IPv6，并说明其序列化形式。

Rule semantics / 规则语义：

- Accept `http` and `https` rules only unless another scheme has an explicitly reviewed need. Parse configuration at save/load time and reject invalid URLs or hosts. / 除非其他协议有经审核的明确需求，只接收 `http` 和 `https` 规则；保存/加载配置时解析，拒绝无效 URL 或主机。
- Store the canonical ASCII host produced by the parser, plus the user's display form for UI. Show both Unicode and ASCII forms for IDNs so an administrator can spot homographs; do not equate visually confusable domains. / 存储解析器生成的规范 ASCII 主机，并保留用户输入的显示形式供 UI 使用。IDN 同时展示 Unicode 和 ASCII 形式，便于管理员识别同形异义域；不得把视觉相似域名视为相同。
- Default to exact-host rules. For an explicit `include_subdomains` rule, match `host == rule` or `host` ends with `.` plus `rule`. [RFC 6265 section 5.1.3](https://www.rfc-editor.org/rfc/rfc6265.html#section-5.1.3) uses the same label-boundary condition for domain matching. Plain `ends_with(rule)` is unsafe because an attacker-controlled longer label can share the suffix. / 默认使用精确主机规则。显式 `include_subdomains` 规则只在 `host == rule`，或 `host` 以 `.` 加 `rule` 结尾时命中。[RFC 6265 第 5.1.3 节](https://www.rfc-editor.org/rfc/rfc6265.html#section-5.1.3) 的域匹配也使用相同标签边界条件。直接 `ends_with(rule)` 不安全，因为攻击者控制的更长标签可能具有相同字符串后缀。
- IP address rules are exact only; never apply subdomain logic to an IP. Compare parsed address values, not user spelling. / IP 地址规则只允许精确匹配；绝不能把子域逻辑用于 IP。比较解析后的地址值，而不是用户书写形式。
- Reject or require an explicit policy for trailing-dot hosts; do not silently strip the dot because WHATWG treats the dotted and undotted forms as distinct. / 对尾点主机应拒绝或要求显式策略；不得静默移除尾点，因为 WHATWG 将有尾点与无尾点形式视为不同。
- Reject `include_subdomains` on a public suffix. RFC 6265 explains why public-suffix rules such as a bare registry suffix cross security boundaries and recommends an up-to-date Public Suffix List. / 禁止在公共后缀上启用 `include_subdomains`。RFC 6265 说明裸注册后缀规则会跨越安全边界，并建议使用最新 Public Suffix List。
- Ignore userinfo, path, query, and fragment for a host-only policy only **after** parsing. This prevents an allowlisted-looking username or query from hiding the actual denylisted host. If path-specific rules are later needed, define a separate typed rule rather than overloading host strings. / 主机级策略只能在完成解析**之后**忽略用户信息、路径、查询和片段，从而避免看似白名单的用户名或查询掩盖真实黑名单主机。将来若需要路径规则，应定义独立的类型化规则，不能复用主机字符串。
- [RFC 3986](https://www.rfc-editor.org/rfc/rfc3986.html#section-6.2.2.1) confirms scheme and host are case-insensitive and should be normalized to lowercase, but browser behavior should follow WHATWG through one parser rather than a home-grown mixture of RFC algorithms. / [RFC 3986](https://www.rfc-editor.org/rfc/rfc3986.html#section-6.2.2.1) 确认协议和主机不区分大小写并应规范为小写；但浏览器行为应通过同一个 WHATWG 解析器实现，而不是自行混合多个 RFC 算法。

## Decision table / 决策表

| Browser URL state / 浏览器 URL 状态 | Title keyword / 标题关键词 | Image score / 图像分数 | Result / 结果 |
| --- | --- | --- | --- |
| allowlisted host / 白名单主机 | any / 任意 | any / 任意 | allow; do not close / 放行，不关闭 |
| denylisted host / 黑名单主机 | any / 任意 | any / 任意 | close without threshold / 不看阈值，关闭 |
| known, neither list / 已知，均未命中 | high-precision match / 高精度命中 | any / 任意 | close without threshold / 不看阈值，关闭 |
| known, neither list / 已知，均未命中 | no match / 未命中 | at or above threshold / 达到阈值 | close / 关闭 |
| known, neither list / 已知，均未命中 | no match / 未命中 | below threshold / 低于阈值 | do not close / 不关闭 |
| unknown or unparsable / 未知或无法解析 | high-precision match / 高精度命中 | any / 任意 | close; never claim an allow/deny match / 关闭；不得声称命中黑白名单 |
| unknown or unparsable / 未知或无法解析 | no match / 未命中 | at or above threshold / 达到阈值 | close / 关闭 |

For non-browser foreground windows, skip URL policy and apply the reviewed title signal before the image threshold. Keep application-level allow rules separate and define their precedence explicitly; this research only fixes the requested website-allowlist invariant. / 对非浏览器前台窗口，跳过 URL 策略，在图像阈值前应用经审核的标题信号。应用级放行规则应独立并明确优先级；本文只固定需求明确提出的网站白名单不变量。

## Minimum regression suite / 最小回归测试集

- allow/deny conflicts, exact host versus subdomain, sibling domains, suffix-collision domains, uppercase input, Unicode IDN/Punycode, userinfo deception, default/non-default ports, trailing dot, canonical IPv4, and IPv6 / 黑白名单冲突、精确主机与子域、同级域、后缀碰撞域、大小写、Unicode IDN/Punycode、用户信息欺骗、默认/非默认端口、尾点、规范 IPv4 与 IPv6；
- browser URL unavailable, stale URL after tab switch, foreground window becoming `NULL`, PID reuse, and focus change between observation and disposition / 浏览器 URL 不可得、切换标签后 URL 过期、前台窗口变为 `NULL`、PID 复用，以及观察与处置之间的焦点变化；
- normalization-equivalent text, full-width forms, mixed Latin/Cyrillic lookalikes, punctuation around tokens, in-word collisions, and long/truncated titles / 规范等价文本、全角形式、拉丁/西里尔混淆字符、词条两侧标点、词内碰撞，以及过长/截断标题；
- Chinese Simplified, Taiwan Traditional, Hong Kong Traditional, Japanese scripts, Russian inflections, and mixed-language titles / 中文简体、台湾正体、香港繁体、日语文字系统、俄语屈折形式和混合语言标题；
- medical, educational, legal, news, historical, identity, and anti-abuse/safety pages as mandatory negative cases / 医学、教育、法律、新闻、历史、身份，以及反滥用/安全页面必须作为反例；
- audit output contains only rule/term IDs, policy version, process identity, and outcome—never the raw URL query, raw title, captured text, or image score / 审计输出仅含规则/词条 ID、策略版本、进程身份和结果；不得包含原始 URL 查询、原始标题、捕获文本或图像分数。

## Source adoption checklist / 来源采用清单

Before any keyword data enters `assets/` or a release bundle: / 任何关键词数据进入 `assets/` 或发布包之前：

1. pin the exact upstream commit and retain the fetched source hash / 固定上游提交并保留抓取源文件哈希；
2. record each retained term's source and why it is adult-content-specific / 记录每个保留词条的来源及其为何足够特指成人内容；
3. obtain native-speaker review and negative-context tests for every supported locale/script / 为每个支持的语言区域/文字系统完成母语者审核和负面语境测试；
4. remove slurs, identity terms, and general profanity unless a reviewed explicit-content phrase genuinely requires them / 删除侮辱语、身份词和通用脏话，除非经审核的明确成人内容短语确实需要；
5. include the applicable license, attribution, and modification notice in source and packaged notices / 在源码与发布包通知中包含适用许可证、署名和修改说明；
6. have legal/provenance review reject any source whose claimed license is not supported by a traceable ownership chain / 对许可证声明缺少可追溯权属链的来源，由法律/来源审核予以拒绝；
7. ship the list version and rollback path independently from executable logic / 词表版本与回滚路径应独立于可执行逻辑发布。

## Assessment of the current Karma notice / 当前 Karma 署名文件评估

The current [`assets/keyword-lists/NOTICE.md`](../assets/keyword-lists/NOTICE.md) correctly links the original LDNOOBW project, identifies CC BY 4.0, and states that Karma reduced broad profanity, normalized casing, and added phrases. This is directionally correct, but it is not yet the strongest form of CC BY attribution. The upstream README supplies a copyright holder and date, and the local notice does not pin the reviewed revision. / 当前 [`assets/keyword-lists/NOTICE.md`](../assets/keyword-lists/NOTICE.md) 正确链接了原 LDNOOBW 项目、标识 CC BY 4.0，并说明 Karma 删除宽泛脏词、规范化大小写和新增短语。方向上正确，但还不是最完整的 CC BY 署名形式。上游 README 提供了版权人和日期，本地通知也没有固定所审核的版本。

Recommended replacement wording, to be applied by the owner of that file rather than by this research task: / 建议由该文件负责人采用以下表述，本调研任务不直接修改：

> Includes material adapted from “Our List of Dirty, Naughty, Obscene, and Otherwise Bad Words,” © 2012–2020 Shutterstock, Inc., revision `5faf2ba42d7b1c0977169ec3611df25a3c08eb13`, licensed under CC BY 4.0. Karma's modifications select a small explicit-adult subset, remove broad profanity and likely false positives, normalize matching forms, and add separately authored multi-word phrases. See the source and license links below. / 包含改编自 “Our List of Dirty, Naughty, Obscene, and Otherwise Bad Words” 的材料，© 2012–2020 Shutterstock, Inc.，版本 `5faf2ba42d7b1c0977169ec3611df25a3c08eb13`，采用 CC BY 4.0。Karma 的修改包括筛选少量明确成人内容词、删除宽泛脏词和高误报候选、规范化匹配形式，并加入独立编写的多词短语。来源与许可证链接见下。

If any retained term was actually copied from dsojevic, MosasoM, fwwdn, Toxicity-200, or LDNOOBW V2 rather than independently authored or derived only from original LDNOOBW, the notice must add that exact source and its license. In particular, do not cite LDNOOBW V2 as CC0 without resolving its upstream chain; do not silently combine CC BY-SA or CC BY-NC-SA data into the current CC BY-only notice. / 如果保留词条实际复制自 dsojevic、MosasoM、fwwdn、Toxicity-200 或 LDNOOBW V2，而不是独立编写或仅由原 LDNOOBW 衍生，则通知必须加入精确来源及其许可证。尤其不能在未解决上游链时把 LDNOOBW V2 简单标为 CC0；也不能把 CC BY-SA 或 CC BY-NC-SA 数据静默并入当前仅写 CC BY 的通知。

This is an engineering and licensing risk assessment, not legal advice. / 本文是工程与许可证风险评估，不构成法律意见。

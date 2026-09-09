# Conventions

## Commits
- Conventional-commit style with a leading emoji: `✨ feat(scope): ...`, `🔒 chore: ...`,
  `📝 docs: ...`, `🔖 bump: vX.Y.Z`, plain `fix(scope): ...` also seen. Scope is usually the
  module touched (`fan`, `rgb`, ...). Body explains *why*, references issue numbers (`#NN`)
  inline rather than in a trailer.
- Code comments and commit prose mix Portuguese and English (repo has PT-BR contributors);
  identifiers/i18n keys are always English snake_case.

## i18n
- All user-visible strings go through `crate::i18n::t("key")`; add the key to every language
  match block in `src/i18n.rs` (currently pt/en/es/zh/ja/ru/de) — a key missing from
  `i18n.rs` compiles fine but falls back to the raw key string at runtime, not an error.

## Hardware-write patterns
- `hardware::helper::execute`/`write_switch`/`read`/`read_switch` are the only sanctioned
  path to privileged EC/sysfs writes (via the `predator-sense-helper` multicall binary,
  through pkexec). Never shell out to sysfs/EC paths directly from GUI code.
- Repeated/periodic privileged calls (e.g. an auto-curve timer) are expensive if each call
  re-spawns pkexec — the helper supports a `--daemon` stdin/stdout mode for exactly this,
  reused across calls by `hardware::helper`'s `execute`/`read_privileged` path; don't
  reintroduce a per-call pkexec spawn in a hot loop.
- `fan::set_fan_mode`/`FanMode::Custom` is intentionally rejected ("disabled for safety") —
  custom PWM only goes through `set_pwm_percent`, never through the firmware-mode helper
  action. Preserve this split when touching fan code.

## README translations
- Root `README.md` (English) is canonical; `README-ptbr.md` and `README-tr.md` are full
  mirrors of its structure, one file per language, no shared includes. Each carries a
  language-switcher line near the top linking the others by relative path.
- Every internal `[text](#anchor)` link must use the *translated* heading's own
  GitHub-slug anchor (lowercased, spaces→hyphens, Turkish/Portuguese diacritics kept as-is,
  parentheses stripped) — not the English anchor. Cross-check after translating headings.
- Code blocks, shell commands, file paths, model codes/table headers-with-codes and URLs
  stay untranslated; only headings, prose and plain-language table cells are translated.
- Docs-only changes (like adding a language) are unrelated to the Rust code — branch them
  off plain `main`, not off any in-flight code-fix branch (contrast with the stacking rule
  below, which is about *code* touching the *same* hot path).

## PR/branch stacking
- When a new feature exercises a code path that an already-open, unmerged PR is actively
  fixing (e.g. a perf/correctness fix in the exact hot loop the new feature makes run more
  often), branch the new feature off that PR's branch rather than off `main`, and say so in
  the new PR's description — avoids shipping a feature that reintroduces/amplifies a bug
  someone already has a fix for in flight.

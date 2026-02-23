# Session Context

## User Prompts

### Prompt 1

I suppose we don't have any sticked convention to use of `Xxxx:new()` and `Xxxx:default()`. We should decide rules for them and documentation the rules for the future session. (And, of course refactor too)

### Prompt 2

[Request interrupted by user]

### Prompt 3

생각을 바꿨어. 가능한 모든 경우에 #[derive(Default)]를 추가할 것. 예외는 생성에 실패할 가능성이 있거나, zero-argument constructing이 불가능할 경우.

기본 방침은 new()를 만들어서 default()로 delegation할 것.
custom default trait구현이 필요할 경우에는 Default trait을 구현할 것.

### Prompt 4

Base directory for this skill: /Users/heejoon.kang/dev/git/fenv-org/mementor/.claude/skills/commit

# Commit Skill

Perform a commit following the project's conventions. Execute the steps below in order.

## 1. Pre-commit checks

1. Run `cargo fmt --check`.
   - If it fails, run `cargo fmt` automatically and include the formatted files in staging.
2. Run `cargo clippy -- -D warnings`.
   - If there are warnings, abort the commit and notify the user of the issues.

## 2. Analyze changes

Run the ...


# Contributing to Hlin

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy --workspace -- -D warnings` and fix all warnings
- No emojis in code, commits, or docs
- Doc comments on all public APIs

## Commit Messages

- Use imperative mood ("add feature" not "added feature")
- Keep the subject line under 50 characters
- No trailing period in subject line
- Reference issues where applicable

## Testing

- Write unit tests for new functionality
- Ensure integration tests pass
- Run the full test suite before submitting: `angreal test all`

## Pull Requests

- Keep PRs focused on a single change
- Include tests for new functionality
- Update documentation as needed

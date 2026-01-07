# Contributing to BlueVault

Thank you for your interest in contributing to BlueVault!

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone <your-fork-url>`
3. Create a branch: `git checkout -b feature/your-feature-name`
4. Make your changes
5. Test your changes: `cargo test && cargo clippy`
6. Commit with clear messages (see below)
7. Push to your fork: `git push origin feature/your-feature-name`
8. Open a pull request

## Code Style

- Run `cargo fmt` before committing
- Fix all `cargo clippy` warnings
- Follow Rust naming conventions
- Document public APIs
- Keep functions focused and testable

## Commit Messages

We prefer clear, descriptive commit messages. Format:

```
Short summary (50 chars or less)

Longer explanation if needed. Explain:
- What changed and why
- Any breaking changes
- Related issues or context
```

Examples:

```
Add dual-mode directory selector with input box and browser

Implements a custom directory browser using ratatui List widget
with lazy loading. Input box is always visible with default focus.
Tab toggles between input and browser modes.

Fixes: Can't tab out of input box
```

```
Implement phosphor green theme system

Adds theme.rs with phosphor, amber, and mono themes. Supports
truecolor RGB with ANSI fallbacks. Theme can be set via TUI_THEME
environment variable.

- Phosphor: Classic green CRT (#3CFF8A on #07110A)
- Amber: Warm amber terminal
- Mono: High-contrast accessibility mode
```

## Testing

- Write tests for new functionality
- Ensure all existing tests pass: `cargo test`
- Test with different terminals (if TUI changes)
- Test with missing optional dependencies

## Pull Request Process

1. Update documentation if needed
2. Add tests for new features
3. Ensure code compiles without warnings
4. Test on a real system if possible
5. Update CHANGELOG.md (if it exists) with your changes

## Questions?

Open an issue for discussion before starting large changes.


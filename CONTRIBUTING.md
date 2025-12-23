# Contributing to Emerald

Thanks for your interest in contributing to Emerald. We welcome issues, discussions, and pull requests from the community.

**The [Telegram group][tg] is available for any concerns you may have that are not covered in this guide.**

## Getting Started

1. Fork the repository and create your branch from `main`.
2. Make sure the project builds and tests pass locally.
3. Keep changes focused and well-scoped.

## Reporting Issues

- Use GitHub Issues to report bugs or request features.
- Search existing issues before opening a new one.
- Provide clear reproduction steps and relevant context.

## Submitting Changes

**Note:** Ideally, a pull request should address an issue that clearly motivate the changes introduced by the pull request.

- Open a pull request with a clear description of the change.
- Reference related issues where applicable.
- Follow existing code style and conventions.
- Add or update tests when appropriate.
- Update documentation if behavior or interfaces change.
- Add a changelog entry (see [Changelog](#changelog) section for details)

### Changelog 

To manage and generate our changelog, we currently use [unclog](https://github.com/informalsystems/unclog).

Every PR with types `fix`, `feat`, `deps`, and `refactor` should include a file 
`.changelog/unreleased/${section}/${pr-number}-${short-description}.md`,
where:

- `section` is one of 
  `dependencies`, `improvements`, `features`, `bug-fixes`, `state-breaking`, `api-breaking`, 
  and _**if multiple apply, create multiple files**_, 
  not necessarily with the same `short-description` or content;
- `pr-number` is the PR number;
- `short-description` is a short (4 to 6 word), hyphen separated description of the change.

For examples, see the [.changelog](.changelog) folder.

Use `unclog` to add a changelog entry in `.changelog` (check the [requirements](https://github.com/informalsystems/unclog#requirements) first): 
```bash
unclog add \
   -i "${pr-number}-${short-description}" \
   -p "${pr-number}" \
   -s "${section}" \
   -m "${description}" \
```
where `${description}` is a detailed description of the changelog entry.

For example, 
```bash
unclog add -i "136-deployment-with-more-nodes" -p 136 -s features -m "Scripts can now generate setup for more than 4 nodes" 
```

**Note:** `unclog add` requires an editor. This can be set either by configuring 
an `$EDITOR` environment variable or by manually specifying an editor binary path 
via the `--editor` flag. 

**Note:** Changelog entries should answer the question: "what is important about this
change for users to know?" or "what problem does this solve for users?". It
should not simply be a reiteration of the title of the associated PR, unless the
title of the PR _very_ clearly explains the benefit of a change to a user.

## Code Review

- All submissions require review before merging.
- Be responsive to feedback and open to discussion.
- Maintainers may request changes to ensure quality and consistency.

## Code of Conduct

By participating, you agree to follow the project's Code of Conduct. 

The Emerald project adheres to the [Rust Code of Conduct][rust-coc]. This code of conduct describes the minimum behavior expected from all contributors.

## License

By contributing, you agree that your contributions will be licensed under the same license as the project.

[rust-coc]: https://rust-lang.org/policies/code-of-conduct/
[tg]: https://t.me/+uHIbcHYVbA44NzNh
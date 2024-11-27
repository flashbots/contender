# Contributing to Contender

Thanks for helping improve the project! We're glad you're here.

There are opportunities to contribute to Contender at any level, whether you're new to Rust or an expert. Every contribution matters and is sincerely appreciated.

This guide will help you get started, but don't feel like you have to read the whole thing all at once! See this as a reference to return to when you're uncertain.

## Conduct

The Contender project adheres to the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct) as a bare minimum, but as a rule of thumb, there are only two requirements when contributing:

- have fun
- respect others

## Contributing Issues

There are three fundamental ways to contribute to issues:

Opening the issue for discussion: If you suspect a bug in Contender, report it by creating a new issue in the issue tracker.

Helping to triage the issue: This involves adding supporting details (e.g., a test case demonstrating the bug), suggesting solutions, or ensuring correct tagging.

Helping to resolve the issue: This can mean proving the issue isn’t a problem or, more often, submitting a Pull Request with specific, reviewable changes to Contender.

Everyone is welcome to engage at any stage. We encourage participation in bug discussions and reviewing PRs.

### Contributions Related to Spelling and Grammar

At this time, we will not be accepting contributions that only fix spelling or grammatical errors in documentation, code or elsewhere.

### Asking for General Help

If you have reviewed existing documentation and still have questions or are having problems, you can open an issue asking for help.

In exchange for receiving help, we ask that you contribute back a documentation PR that helps others avoid the problems that you encountered.

### Submitting a Bug Report

When opening a new issue, users will be presented with a basic [template](.github/PULL_REQUEST_TEMPLATE.md) that should be filled in. If you believe that you have uncovered a bug, please fill out this form, following the template to the best of your ability. Do not worry if you cannot answer every detail, just fill in what you can.

The two most important pieces of information we need in order to properly evaluate the report is a description of the behavior you are seeing and a simple test case we can use to recreate the problem on our own. If we cannot recreate the issue, it becomes impossible for us to fix.

In order to rule out the possibility of bugs introduced by userland code, test cases should be limited, as much as possible, to using only Contender APIs.

See [How to create a Minimal, Complete, and Verifiable example](https://stackoverflow.com/help/minimal-reproducible-example).

### Triaging a Bug Report

Once an issue has been opened, it is not uncommon for there to be discussion around it. Some contributors may have differing opinions about the issue, including whether the behavior being seen is a bug or a feature. This discussion is part of the process and should be kept focused, helpful, and professional.

Short, clipped responses -- that provide neither additional context nor supporting detail -- are not helpful or professional. To many, such responses are simply annoying and unfriendly.

Contributors are encouraged to help one another make forward progress as much as possible, empowering one another to solve issues collaboratively. If you choose to comment on an issue that you feel either is not a problem that needs to be fixed, or if you encounter information in an issue that you feel is incorrect, explain why you feel that way with additional supporting context, and be willing to be convinced that you may be wrong. By doing so, we can often reach the correct outcome much faster.

### Resolving a Bug Report

In the majority of cases, issues are resolved by opening a Pull Request. The process for opening and reviewing a Pull Request is similar to that of opening and triaging issues, but carries with it a necessary review and approval workflow that ensures that the proposed changes meet the minimal quality and functional guidelines of the Contender project.

## Pull Requests

Pull Requests are the primary way to propose changes to Contender's code, documentation, or dependencies.

Before making significant changes, it's best to open an issue first to gather feedback and guidance. This improves the chances of your PR being accepted.

When submitting a PR, enable the "Allow Edits From Maintainers" option. Contender enforces strict standards for code quality, style, and commit signing. Allowing edits lets maintainers make minor adjustments to align your PR with these standards, helping it get merged faster with less effort on your part.

### Cargo Commands

Here are some common commands we'll use:

```sh
cargo check --workspace
cargo +nightly fmt --all
cargo build --workspace
cargo test --workspace
cargo +nightly clippy --workspace
```

> 💡 you need [anvil from Foundry](https://book.getfoundry.sh/) and [sqlite3]() to run the tests

### Tests

If your proposed change modifies code (rather than just documentation), it should either add new functionality to Contender or fix broken functionality. In both cases, the pull request must include tests to prevent future regressions.

#### Unit Tests

Functions with specific tasks should have unit tests. We recommend using table-driven tests to cover many cases clearly and concisely.

#### Integration Tests

Place integration tests in the tests/ directory of the crate containing the code being tested. For guidance, review existing integration tests in the same crate and follow their style.

#### Documentation Tests

Every API should ideally include at least one [documentation test](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html) demonstrating its usage. Run these tests using `cargo test --doc` to ensure examples are accurate and provide extra test coverage.

When writing documentation tests, balance clarity for readers with effective API testing. Use the `/// #` prefix to include lines necessary for the test but exclude them from the generated documentation, keeping user-facing examples clean and concise.

### Commits

It is a recommended best practice to keep your changes as logically grouped as possible within individual commits. There is no limit to the number of commits any single Pull Request may have, and many contributors find it easier to review changes that are split across multiple commits.

That said, if you have a number of commits that are "checkpoints" and don't represent a single logical change, please squash those together.

Note that multiple commits often get squashed when they are landed (see the notes about commit squashing).

#### Commit message guidelines

Commit messages should follow the [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) specification.

Here are a few examples:

* feat(abigen): support empty events
* chore: bump crypto deps
* test: simplify test cleanup
* fmt: run rustfmt

### Opening the Pull Request

From within GitHub, opening a new Pull Request will present you with a [template](.github/PULL_REQUEST_TEMPLATE.md) that should be filled out. Please try to do your best at filling out the details, but feel free to skip parts if you're not sure what to put.

### Discuss and Update

You will probably get feedback or requests for changes to your Pull Request. This is a big part of the submission process so don't be discouraged! Some contributors may sign off on the Pull Request right away, others may have more detailed comments or feedback. This is a necessary part of the process in order to evaluate whether the changes are correct and necessary.

Any community member can review a PR and you might get conflicting feedback. Keep an eye out for comments from code owners to provide guidance on conflicting feedback.

Once the PR is open, do not rebase the commits. See [Commit Squashing](#commit-squashing) for more details.

### Commit Squashing

In most cases, do not squash commits that you add to your Pull Request during the review process. When the commits in your Pull Request land, they may be squashed into one commit per logical change. Metadata will be added to the commit message (including links to the Pull Request, links to relevant issues, and the names of the reviewers). The commit history of your Pull Request, however, will stay intact on the Pull Request page.

## Reviewing Pull Requests

Any Contender community member is welcome to review pull requests.

Contributors who provide feedback on PRs have a responsibility to both the project and the individual submitting the contribution. Feedback should aim to improve the PR constructively rather than block it without reason. If you believe a PR shouldn't be merged, explain your reasoning clearly. Be open to having your perspective changed and to collaborating with the contributor to enhance the submission.

Disrespectful or dismissive reviews violate the Code of Conduct. 

### Goals of a Review

The primary objectives are to improve Contender's codebase and support the contributor's success. Even if a PR doesn't get merged, the contributor should feel their effort was valued and worthwhile. Each PR, especially from new contributors, is an opportunity to grow the community.

- Review incrementally and avoid overwhelming new contributors.
- Focus on significant aspects first:
  - Does the change align with Contender's goals?
  - Does it improve Contender, even incrementally?
  - Are there clear bugs or major issues?
  - Is the commit message clear and accurate, especially for breaking changes?

Incremental improvement is sufficient for merging a PR; it doesn’t need to be perfect. Further improvements can be addressed in follow-up PRs.

### Giving Feedback

When requesting changes, frame them as requests, not demands. Assume the contributor may need guidance on tasks like adding tests or benchmarks. Avoid prioritizing minor issues like micro-optimizations, grammar, or strict style adherence unless they are significant. 

For non-essential changes ("nits"), clearly mark them as such (e.g., *Nit: change foo() to bar(). This isn’t blocking*). Avoid stalling PRs over minor fixes, as collaborators can often address these when merging. 

If a comment becomes irrelevant after updates or turns out to be mistaken, [hide it](https://docs.github.com/en/communities/moderating-comments-and-conversations/managing-disruptive-comments#hiding-a-comment) with an appropriate reason to keep discussions concise.

### Respect the Contributor

Be mindful that how you communicate impacts the contributor's experience. While the goal is to improve Contender, it’s equally important to ensure contributors feel encouraged to engage again.

### Handling Abandoned or Stalled PRs

If a PR appears stalled or abandoned, check with the contributor before taking over the work. If they agree, credit their efforts—either by retaining their name and email in the commit log or using an *Author:* metadata tag.

---

*Adapted from [Alloy contributing guide](https://github.com/alloy-rs/alloy/blob/main/CONTRIBUTING.md)*.

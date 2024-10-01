# Contributing to Pilot Training Data Synchronization Project

Thank you for your interest in contributing to Pilot Training Data Synchronization! We welcome contributions from the community to help improve and enhance the project. This document outlines the guidelines, steps, and rules for contributing to the project.

## Contribution Rules

To ensure a smooth and collaborative contribution process, please follow these rules:

1. Be respectful and considerate towards other contributors and maintainers.
2. Before starting work on a significant feature or change, discuss it with the maintainers through an issue or proposal to align with the project's goals and avoid duplication of efforts.
3. Follow the project's coding conventions, style guidelines, and best practices.
4. Write clear, concise, and meaningful commit messages.
5. Keep pull requests focused on a single logical change or feature.
6. Provide thorough and constructive feedback when reviewing others' contributions.
7. Be open to feedback and suggestions from maintainers and other contributors.
8. Do not include any sensitive, confidential, or proprietary information in your contributions.
9. By submitting a contribution, you agree to license your work under the project's chosen license.

## Raising Bugs

If you encounter a bug or an issue while using, please follow these steps to report it:

1. Check the existing issues in the [issue tracker](https://github.com/oss-slu/PilotDataSynchronization/issues) to see if the bug has already been reported.
2. If the bug hasn't been reported, open a new bug.
3. Provide a clear and descriptive title for the issue.
4. Include steps to reproduce the bug, along with any relevant error messages or screenshots.
5. Specify the environment (operating system, browser, etc.) where the bug occurs.

## Proposing New Features

If you have an idea for a new feature or enhancement, we encourage you to propose it to the project. Here's how you can do it:

1. Check the existing issues in the [issue tracker](https://github.com/oss-slu/PilotDataSynchronization/issues) to see if it already exists. 
2. Open a new issue.
2. Provide a clear and descriptive title for the feature proposal.
3. Explain the motivation and benefits of the proposed feature.
4. Describe the proposed solution or implementation, if you have one in mind.
5. Be open to discussions and feedback from the project maintainers and the community.

## Contributing to Pull Requests

To contribute code changes, please follow these steps:

1. Fork the repository and create a new branch for your changes.
2. Make your code changes in the new branch.
3. Write clear and concise commit messages for each logical change.
4. Make sure your code follows the project's coding conventions and style guidelines.
5. Write unit tests to cover your code changes and ensure they pass.
6. Update the relevant documentation, including README files and code comments, if necessary.
7. Push your changes to your forked repository.
8. Open a pull request against the main repository's `main` branch.
9. Provide a clear and descriptive title for the pull request.
10. Include a detailed description of the changes made and the problem they solve.
11. Reference any related issues or feature requests in the pull request description.

### Getting Started:

- Fork this repo
- Clone on your local machine

```terminal
git clone https://github.com/oss-slu/PilotDataSynchronization.git
```

- Create a new Branch

```
git checkout -b my-new-branch
```
- Add your changes
```
git add .
```
- Commit your changes.

```
git commit -m "Relevant message"
```
- Then push 
```
git push origin my-new-branch
```


- Create a new pull request from your forked repository

<br>

### Avoid Conflicts {Syncing your fork}

An easy way to avoid conflicts is to add an 'upstream' for your git repo, as other PR's may be merged while you're working on your branch/fork.   

```
git remote add upstream https://github.com/oss-slu/PilotDataSynchronization
```

You can verify that the new remote has been added by typing
```
git remote -v
```

To pull any new changes from your parent repo simply run
```
git merge upstream/main
```

This will give you any eventual conflicts and allow you to easily solve them in your repo. It's a good idea to use it frequently in between your own commits to make sure that your repo is up to date with its parent.

## Documenting Changes

When making changes to the codebase, it's important to keep the documentation up to date. Here are some guidelines for documenting your changes:

- Update the relevant README files, documentation, or user guides to reflect the changes made.
- Add or update code comments to explain complex or non-obvious parts of the code.
- Include any necessary configuration or setup instructions for new features or changes.

## Writing Unit Tests

We encourage contributors to write unit tests for their code changes to ensure the stability and reliability of the project. When writing unit tests:

- Use the project's testing framework and conventions.
- Write tests that cover the various scenarios and edge cases related to your code changes.
- Ensure that the tests pass successfully before submitting a pull request.

## Contacting the Team

If you have any questions, concerns, or need further assistance, you can join the following Open Source with SLU slack workspace and then join #project_pilot_training_data_synchronization channel:

[Slack workspace](https://join.slack.com/t/oswslu/shared_invite/zt-24f0qhjbo-NkSfQ4LOg5wXxBdxP4vzfA)

[Link to channel](https://oss-slu.slack.com/archives/C07JBA6V003)

## Community Partners

You can refer to the [Community Partners](https://oss-slu.github.io/docs/about/community) for more details guidance on contributing to the project.

We appreciate your contributions and look forward to collaborating with you!

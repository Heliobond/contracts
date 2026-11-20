<!-- Keep PRs focused — one issue each. -->

## Summary

<!-- What does this change and why? -->

## Linked issue

Closes #<!-- issue number -->

## Type of change

- [ ] Bug fix
- [ ] Feature
- [ ] Tests
- [ ] Docs / chore

## Checklist

- [ ] Tied to an accepted issue (`Closes #…`)
- [ ] All quality-gate checks pass locally (see [CONTRIBUTING.md — Before you push](../CONTRIBUTING.md#before-you-push))
- [ ] New behaviour is covered by tests
- [ ] No secrets committed
- [ ] Docs updated where relevant

## Security note (fund-flow changes only)

<!-- Fill in if this PR touches deposit / withdraw / fund_project / share math. Otherwise delete this section. -->

- Share math: division denominators cannot be zero; rounding favours vault
- Fund flows: tokens transferred in correct direction, balances updated atomically
- Access control: auth guard fires before any state change

# Fungi skills

Skills for installing and operating [Fungi](https://github.com/enbop/fungi), a private multi-device service platform.

The `skills/` directory uses the standard `SKILL.md` format and can be installed into any agent supported by the [`skills` CLI](https://github.com/vercel-labs/skills#supported-agents), including Claude Code, Codex, Cursor, Gemini CLI, OpenCode, and many others.

## Install

List the skills available in the Fungi repository:

```bash
npx skills@latest add enbop/fungi --list
```

Install the Fungi skill and let the CLI detect or prompt for available agents:

```bash
npx skills@latest add enbop/fungi --skill fungi
```

Install it globally instead of in the current project:

```bash
npx skills@latest add enbop/fungi --skill fungi --global
```

Optionally target one or more agents explicitly using their supported agent IDs:

```bash
npx skills@latest add enbop/fungi --skill fungi --agent AGENT_ID
```

Product-specific metadata under `agents/` is optional. The Fungi instructions and references remain portable across compatible agents.

## Included skills

- `fungi`: Install and initialize Fungi, connect devices with explicit trust approval, manage local and remote services, apply recipes, create `.fungi.md` service files, and diagnose results with inspect and bounded logs.

## Migrate an existing installation

If you installed from `enbop/fungi-skills`, run the new `add` command with the same installation scope and agent selection, and review the overwrite prompt. For example, for a global installation:

```bash
npx skills@latest add enbop/fungi --skill fungi --global
```

Preserve any local customizations before replacing the installed skill. Running `skills update` alone continues to use the recorded old source; it does not switch repositories. Check that the resulting skill lock entry records `enbop/fungi`. For project installations, include the updated `skills-lock.json` in your project's changes.

The former repository's existing links and installations remain a separate transition concern; moving the source here does not redirect them automatically.

## Maintain alongside the CLI

Keep CLI behavior and skill instructions aligned in the same pull request. These instructions target this repository's current CLI; agents must check the installed version and command help, especially when using an older released binary. Document version differences when introducing commands not yet available in released builds. Skill-only documentation changes do not require a new binary release.

The skill was imported from [enbop/fungi-skills](https://github.com/enbop/fungi-skills) at commit `c02d2b662eb835b731aeb407126cb5a51736b043`, including the service lifecycle and trust direction fixes from PR #5. Maintenance and feedback now belong in this repository.

## Feedback

Found a problem or have a suggestion?

- Fungi CLI, daemon, or service behavior: [enbop/fungi issues](https://github.com/enbop/fungi/issues)
- Fungi App behavior: [enbop/fungi-app issues](https://github.com/enbop/fungi-app/issues)
- Skill instructions: [enbop/fungi issues](https://github.com/enbop/fungi/issues)

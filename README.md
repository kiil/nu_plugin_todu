# Todu Plugin

A [Nushell](https://www.nushell.sh/) plugin for managing project-scoped todos, with optional sync to GitHub and Jira.

## Features

- Intuitive inline parsing for quick task creation with natural-language dates

```nushell
todu add "!!write tests for backend #tests @tomorrow // need more test cases for feature"
╭───┬─────────────────────────┬─────────┬──────────┬──────┬────────────┬──────────┬───────╮
│ # │          task           │ status  │ priority │ desc │    due     │ subtasks │  tag  │
├───┼─────────────────────────┼─────────┼──────────┼──────┼────────────┼──────────┼───────┤
│ 1 │ write tests for backend │ pending │ medium   │ ...  │ in 8 hours │ ---      │ tests │
╰───┴─────────────────────────┴─────────┴──────────┴──────┴────────────┴──────────┴───────╯
```

- Integrates with Nushell pipelines for powerful task management

```nushell
todu | sort-by status
todu | group-by tag? 
todu | where priority? > medium | get task desc
todu | where status == pending | get id | todu tag work
todu add "refactor auth module" | [$"write unit tests ^($in.id)" $"update docs ^($in.id)"] | todu add
```

- Taskwarrior-style urgency scoring, with `todu next` for the most urgent actionable todos — [see Urgency below](#urgency)
- Optional impact judgments from [jev](#jev-integration) (TypeSafe System One) that feed into urgency

- Pull GitHub issues and Jira tasks into your project list, with status changes pushed back automatically (requires `--features remote`) — [see Remote setup below](#remote-setup)

## Installation

### From source

```nushell
# Local todos only
cargo build --release
plugin add target/release/nu_plugin_todu
plugin use todu

# With GitHub and Jira support
cargo build --release --features remote
plugin add target/release/nu_plugin_todu
plugin use todu
```

### Requirements

- Nushell 0.116+
- Rust toolchain (for building from source)

## Urgency

Every live todo gets an `urgency` score, computed like
[Taskwarrior's](https://taskwarrior.org/docs/urgency/): each factor's value is multiplied by a
coefficient and the products are summed.

| factor      | todu source                                     | default |
|-------------|-------------------------------------------------|---------|
| `due`       | due date, 0.2 at 14+ days out → 1.0 at 7 days overdue | 12.0 |
| `blocking`  | subtask (it blocks its parent)                  | 8.0     |
| `priority`  | high / medium / low                             | 6.0 / 3.9 / 1.8 |
| `impact`    | judged impact 0.0–1.0 (`todu impact`)           | 6.0     |
| `active`    | status `in-progress`                            | 4.0     |
| `age`       | age / `max_age` (365 days), capped at 1         | 2.0     |
| `desc`      | has a description                               | 1.0     |
| `tags`      | has a tag                                       | 1.0     |
| `tag.<t>`   | per-tag coefficient (`next` is 15.0)            | —       |
| `waiting`   | status `paused`                                 | -3.0    |
| `blocked`   | has unfinished subtasks                         | -5.0    |

Done and stopped todos have zero urgency.

```nushell
todu --urgency                 # sort the tree by urgency
todu next -n 5                 # the 5 most urgent actionable todos, subtasks included
todu urgency 3                 # explain the score of #3, factor by factor
todu urgency                   # show the coefficients in effect
todu impact 0.8 3              # set a judged impact by hand
todu | where urgency > 10
```

Override coefficients in the plugin config:

```nushell
$env.config.plugins.todu = {
    urgency: { due: 10.0, priority_high: 7.0, tag: { next: 20.0, someday: -6.0 } }
}
```

## jev integration

[`nu/todu-jev`](nu/todu-jev/mod.nu) is a Nushell module that asks the
`jev` module (TypeSafe System One, expected at `<config-dir>/modules/jev`) how much each todo
matters, stores the answer with `todu impact`, and so ranks `todu next` by it. Impact is judged
apart from due dates and priority, which have their own factors.

```nushell
use nu/todu-jev
todu-jev assess                      # rate actionable todos that have no impact yet
todu-jev assess 3 7 --priority       # rate #3 and #7, and set priority where missing
todu-jev assess --force --context "Launch is next week; stability first."
todu-jev add "fix login redirect loop #auth @friday"   # add, then rate
todu-jev next -n 5                   # rate what is unrated, then show `todu next`
todu-jev assess --dry-run            # show the jev requests without sending
```

`$env.TODU_JEV_CONTEXT` supplies the default `--context`.

## Usage

See the [wiki](https://github.com/casedami/nu_plugin_todu/wiki) for details.

## Contributing

Contributions are welcome. Feel free to open a PR or create an issue.

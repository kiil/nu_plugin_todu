# Judge todu tasks with jev, and feed the judgment into urgency
#
# jev (the TypeSafe System One module) rates how much each todo matters. The
# rating is stored with `todu impact` and weighted into the urgency score by the
# `impact` coefficient, so `todu next` and `todu --urgency` rank by it.
#
# Impact is judged apart from deadlines and priority on purpose: due dates and
# priority already have their own urgency factors, and asking the model about
# them again would count them twice.
#
# $env.TODU_JEV_CONTEXT  what matters in this project right now, handed to the
#                        model with every todo. `--context` overrides it.
#
# Requires the todu plugin and the jev module in <config-dir>/modules/jev.

use ($nu.default-config-dir | path join modules jev)

const IMPACT_LEVELS = [
    "Nothing much happens if it is never done"
    "A small convenience or nice-to-have"
    "Noticeably improves the project, or unblocks a little other work"
    "Important to users or to the project's goals, or unblocks a lot of other work"
    "Something is broken, insecure, losing data, or people are blocked right now"
]

const PRIORITIES = {
    low: "Can wait; do it when there is slack"
    medium: "Should get done in the normal course of work"
    high: "Should be done before most other things"
}

# A todu command's output as a list of rows. Single results come back as a
# record, and an empty list as a message string.
def as-rows []: any -> list<record> {
    match ($in | describe --detailed | get type) {
        "list" => $in
        "record" => [$in]
        _ => []
    }
}

def project-name [global: bool]: nothing -> string {
    if $global { return "personal" }
    let root = do { ^git rev-parse --show-toplevel } | complete
    if $root.exit_code == 0 { $root.stdout | str trim | path basename } else { $env.PWD | path basename }
}

# What the model sees for one todo
def state-of [todo: record, project: string, context: any]: nothing -> record {
    {
        project: $project
        context: $context
        task: ($todo.title | ansi strip)
        description: $todo.desc?
        tag: $todo.tag?
        status: ($todo.status | into string | ansi strip)
        subtasks: ($todo.subtasks? | default [] | each { $in.title | ansi strip })
    } | compact --empty
}

def questions [with_priority: bool]: nothing -> record {
    let impact = jev score "How much does getting this task done matter? Judge the consequence of doing it, not when it is due." $IMPACT_LEVELS
    if $with_priority {
        { impact: $impact, priority: (jev choice "What priority should this task have?" $PRIORITIES) }
    } else {
        { impact: $impact }
    }
}

# Rate todos with jev and store the impact, so urgency reflects it
#
# Without ids, rates the actionable todos (those `todu next` lists) that have no
# impact yet. Ids can be given as arguments or piped in.
@example "rate every unrated actionable todo" { todu-jev assess }
@example "rate two todos, and set their priority too" { todu-jev assess 3 7 --priority }
@example "re-rate the tagged todos with project context" {
    todu | where tag? == backend | get id | todu-jev assess --force --context "Launch is next week; stability first."
}
@example "see the requests without sending them" { todu-jev assess --dry-run }
export def assess [
    ...ids: int                 # Todo ids (or pipe ids in)
    --force (-f)                # Re-rate todos that already have an impact
    --priority (-p)             # Also let jev set priority, for todos without one (all with --force)
    --context (-c): string      # What matters right now. Default: $env.TODU_JEV_CONTEXT
    --model (-m): string        # jev model
    --global (-g)               # Use the home directory as project
    --dry-run                   # Return the jev requests and change nothing
]: [nothing -> any, int -> any, list<int> -> any] {
    let piped = $in
    let ids = $ids | append ($piped | default []) | uniq
    let project = project-name $global
    let context = $context | default $env.TODU_JEV_CONTEXT?

    let todos = if ($ids | is-empty) {
        todu next --limit 100000 --global=$global
        | as-rows
        | each {|t| todu get $t.id --global=$global }
        | where {|t| $force or $t.impact? == null }
    } else {
        $ids | each {|id| todu get $id --global=$global }
    }
    if ($todos | is-empty) { return [] }

    let with_priority = $priority
    let qs = questions $with_priority

    if $dry_run {
        return ($todos | each {|t|
            { id: $t.id, request: (state-of $t $project $context | jev ask $qs --dry-run --model=$model) }
        })
    }

    let answers = jev with-key {
        $todos | par-each --keep-order {|t|
            { todo: $t, answer: (state-of $t $project $context | jev ask $qs --model=$model) }
        }
    }

    let max_level = ($IMPACT_LEVELS | length) - 1
    $answers | each {|a|
        let id = $a.todo.id
        let impact = $a.answer.impact.score / $max_level | math round --precision 2
        todu impact ($impact | into string) $id --global=$global | ignore

        let set_priority = $with_priority and ($force or "priority" not-in ($a.todo | columns))
        if $set_priority {
            todu priority $a.answer.priority.choice $id --global=$global | ignore
        }

        let after = todu get $id --global=$global
        {
            id: $id
            title: ($after.title | ansi strip)
            impact: $impact
            confidence: ($a.answer.impact.confidence | math round --precision 2)
            priority: $after.priority?
            urgency: $after.urgency
        }
    }
}

# Add a todo with todu's inline syntax, then rate it with jev
@example "add and rate" { todu-jev add "!!fix login redirect loop #auth @friday" }
export def add [
    text: string                # Todo text, parsed like `todu add`
    --priority (-p)             # Also let jev set priority, if the text gives none
    --context (-c): string      # What matters right now. Default: $env.TODU_JEV_CONTEXT
    --model (-m): string        # jev model
    --global (-g)               # Use the home directory as project
]: nothing -> any {
    let todo = todu add $text --global=$global
    $todo.id | assess --priority=$priority --context=$context --model=$model --global=$global
}

# Rate the unrated todos, then show the most urgent ones
@example "what should I do next?" { todu-jev next -n 5 }
export def next [
    --limit (-n): int = 10      # Maximum number of todos to show
    --context (-c): string      # What matters right now. Default: $env.TODU_JEV_CONTEXT
    --model (-m): string        # jev model
    --global (-g)               # Use the home directory as project
]: nothing -> any {
    assess --context=$context --model=$model --global=$global | ignore
    todu next --limit $limit --global=$global
}

# Judge todu tasks with jev, and feed the judgment into urgency
export def main []: nothing -> table {
    scope commands
    | where name starts-with "todu-jev "
    | select name description
}

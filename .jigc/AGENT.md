`jigc` is your interface to this project — your single, current source for the workflow for your task, the project's state, and the doc context you need, all assembled and validated for you. The files are storage, not your interface: never read or edit managed docs directly. Start every task with `jigc start`; write every change back through `jigc`.

`jigc` is a context compiler: it assembles exactly the workflow steps and doc slices your task needs, and owns every structural write — placement, cross-references, commits. You author only the prose.

Read every command's output; a non-zero exit means stop and follow what the output says — never retry blindly.

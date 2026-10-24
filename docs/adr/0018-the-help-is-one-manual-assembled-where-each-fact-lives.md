# The help is one Markdown manual, assembled from where each fact lives

wxread's help was text files: a guide (`help.txt`) for `--help`, a screen (`short.txt`) for `-h`, and one file a command (`help/<command>.txt`), each glued to its command by a hand-kept match, wrapped by hand at 72 columns, the numbers and lists they quoted copied from the code (#87). It is now one Markdown manual that [agent-help](https://github.com/sunfmin/agent-cli/tree/main/crates/agent-help) assembles, as every agent-cli tool's is.

## How

- **A command's help is its doc comment**, on its variant in `crates/cli/src/command.rs`, kept as written (`verbatim_doc_comment`); an argument's is the doc comment on its field. Its first paragraph is the line `-h` lists it by.
- **Who runs a command is its group**, `next_help_heading`, declared where it is defined; the manual lists the commands under their groups.
- **A command's help is written in parts**, each under a label on a line of its own (`**When to run it**`, `**What it reads**`, `**What it answers**`, `**Trips you up**`, …), not under headings: the same text is the top of `wxread <command> --help` and a section four levels down in the manual, and a heading cannot sit right in both. The keys of an answer are a list, `- \`key\`: …`, the keys of a key's value nested under it.
- **A number or a list the help quotes from code is a placeholder**, `{const:NAME}`, that `command::manual` fills from the code: the page limits, the seconds `group leave` and `keys` wait, the folders of WeChat's chat list, the favorite types. A list whose items each carry help (the kinds of a message and of a favorite, what search reads of each kind, the folders) is written by an exhaustive `match` in the effect module that answers it, so a kind added without its help does not compile.
- **What belongs to no command is a topic**, `wxread help <topic>`: how every person an answer names is `{id, display_name}` (`names`).
- **`--help` is the whole manual, `<command> --help` that command's part, `-h` one screen** built from each command's first paragraph and the global arguments' help. `agent_help::check` holds the manual whole; a test holds that each command's answer is its part of the manual.

## Considered

- **`agent-help` in the pure crates, so `media::Refusal`, `sqlcipher::Refusal` and the kinds could be `#[derive(agent_help::Help)]` tables.** A table's help would sit on the variant itself. But agent-help brings clap into crates that do no I/O and pass Flux, and wxread never prints these codes: a refusal becomes an error sentence that names its fix (`wxread_media::image_refused`, the account crate's), and that sentence is what a reader meets. The help talks about those errors where they happen (each command's Trips you up), and the kinds' help is the effect crate's exhaustive `match`.
- **Headings inside a command's help.** Right in `<command> --help` or in the manual, never both.

## Consequences

- No help is a text file and nothing is wrapped: a paragraph is one line, and a test fails on one that is not.
- A number the code changes changes in the help with it; a kind, a folder or a command added without help fails to build, or fails `check`.
- An example the manual shows that a test runs is taken from the manual's code blocks (`agent_help::code_blocks`): the tests supply keys in the shape `wxread keys --help` shows, and run the lines it shows for supplying them, so an example cannot drift from one that works.
- `wxread help` is a command, and prints text, not JSON, as `--help`, `-h` and `--version` do.

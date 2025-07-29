use crate::util;
use crate::fm::FileManager;
use crate::todo::{Todos, Description};

use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub struct Prompt {
    pub todos: Todos
}

pub struct Prompter {
    pub fm: Arc<FileManager>,
    pub issue_tx: UnboundedSender<Todos>,
}

impl Prompter {
    make_spawn!{
        Prompt,
        #[inline]
        pub fn new(
            fm: Arc<FileManager>,
            issue_tx: UnboundedSender<Todos>,
        ) -> Self {
            Self { fm, issue_tx }
        }
    }

    pub async fn run(&self, mut prompter_rx: UnboundedReceiver<Prompt>) {
        while let Some(p) = prompter_rx.recv().await {
            let Some(file_id) = p.todos.first().map(|t| t.loc.file_id()) else {
                continue
            };

            let mut todos = p.todos.into_vec();

            let file_name = self.fm.get_file_path_unchecked(file_id);

            let to_report = loop {
                util::clear_screen();

                println!("[todoʼs from]: {file_name}\n");

                // build all prefixes and find max width to pad all titles
                let prefixes = todos
                    .iter()
                    .enumerate()
                    .map(|(i, todo)| format!{
                        "{n}. [line {line}]:",
                        n = i + 1,
                        line = todo.loc.line_number()
                    }).collect::<Vec<_>>();

                let max_width = prefixes.iter().map(|p| p.len()).max().unwrap_or(0);

                // print each line with padding
                for (i, (todo, prefix)) in todos.iter().zip(prefixes.iter()).enumerate() {
                    // how many spaces to add so that all titles line up
                    let pad = max_width - prefix.len();

                    // pad BEFORE the space between colon and title
                    println!{
                        "{prefix}{tab} {title}",
                        tab = " ".repeat(pad),
                        title = todo.title
                    };

                    if let Some(desc) = &todo.description {
                        println!("   └── description:\n{d}", d = desc.display(9));
                        if i < todos.len() - 1 { println!() }
                    }
                }

                println!();

                let cmd = util::ask_input("selection (e.g. 1,2; 'q' skip; 'h' help):");
                let cmd = cmd.trim();

                if cmd.eq_ignore_ascii_case("q") { break None }
                if cmd.eq_ignore_ascii_case("h") { Self::print_help(); continue }

                // editing mode
                if let Some(pos) = cmd.find(|c: char| !c.is_ascii_digit()) {
                    let (num_str, flags) = cmd.split_at(pos);

                    if let Ok(idx) = num_str.parse::<usize>() {
                        let i = idx.wrapping_sub(1);

                        if i >= todos.len() {
                            println!("invalid index.");
                            continue
                        }

                        let todo = &mut todos[i];

                        let mut any = false;

                        let edit_flags = flags.trim();
                        if edit_flags.contains('t') {
                            let new_title = util::ask_input("enter new title:");
                            let new_title = new_title.trim();

                            todo.title = util::string_into_boxed_str_norealloc(
                                new_title.to_owned()
                            );

                            any = true;
                        }

                        if edit_flags.contains('d') {
                            let new_desc = util::ask_input(
                                "enter new description (leave empty to remove):"
                            );
                            let new_desc = new_desc.trim();

                            todo.description = if new_desc.is_empty() {
                                None
                            } else {
                                Some(Description::from_str(new_desc))
                            };

                            any = true;
                        }

                        if any {
                            // re‑print updated list
                            continue
                        }
                    }
                }

                // selection mode
                let to_report = if cmd.eq_ignore_ascii_case("all") {
                    todos
                } else {
                    let mut report_indexes = cmd.split(',').filter_map(|s| {
                        s.trim().parse::<usize>().ok().map(|n| n.wrapping_sub(1))
                    }).filter(|i| *i < todos.len()).collect::<Vec<_>>();

                    if report_indexes.is_empty() {
                        println!("no valid selections.");
                        continue
                    }

                    report_indexes.sort_unstable();
                    report_indexes.dedup();

                    let mut to_report = report_indexes.into_iter()
                        .rev()
                        .map(|index| todos.remove(index))
                        .collect::<Vec<_>>();

                    // restore original order
                    to_report.reverse();
                    to_report
                };

                let to_report = util::vec_into_boxed_slice_norealloc(to_report);

                break Some(to_report)
            };

            if let Some(to_report) = to_report {
                self.issue_tx
                    .send(to_report)
                    .expect("could not send todos to issue worker");
            }
        }
    }

    #[inline]
    fn print_help() {
        const HELP_TEXT: &str = r#"
HELP:
 • enter comma-separated indices to select todos, e.g. 1,3,5
 • all -> select all
 • q   -> skip this file entirely
 • h   -> show this help screen
 • prefix with:
     t        -> edit title       (e.g. 6t)
     d        -> edit description (e.g. 6d)
     td or dt -> edit both        (e.g. 4td)
"#;

        println!("{HELP_TEXT}");
        _ = util::ask_input("press <enter> to continue ..");
    }
}

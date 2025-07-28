use crate::util;
use crate::fm::FileManager;
use crate::todo::{Todo, Todos};

use std::mem;
use std::sync::Arc;
use std::fmt::Write as FmtWrite;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub struct Prompt {
    pub todos: Todos
}

pub struct Prompter {
    pub fm: Arc<FileManager>,

    pub prompter_rx : UnboundedReceiver<Prompt>,
    pub issue_tx    : UnboundedSender<Todos>,
}

impl Prompter {
    pub async fn prompt_loop(&mut self) {
        let mut stdout_buf = String::new();

        while let Some(p) = self.prompter_rx.recv().await {
            let to_report = p.todos.into_iter().filter_map(|t| {
                let Todo {
                    loc,
                    preview,
                    title,
                    description,
                    ..
                } = &t;

                writeln!{
                    stdout_buf,
                    "found TODO at {l}: {preview}",
                    l = loc.display(&self.fm)
                }.ok()?;

                writeln!{
                    stdout_buf,
                    "  title: \"{title}\""
                }.ok()?;

                if let Some(desc) = &description {
                    writeln!{
                        stdout_buf,
                        "  description:\n{d}",
                        d = desc.display(4)
                    }.ok()?;
                }

                println!("{p}", p = mem::take(&mut stdout_buf));

                let should_report = util::ask_yn("report it?");

                if should_report { Some(t) } else { None }
            }).collect::<Vec<_>>();

            if to_report.is_empty() {
                continue
            }

            let to_report = util::vec_into_boxed_slice_norealloc(to_report);

            self.issue_tx
                .send(to_report)
                .expect("could not send todos to issue worker");
        }
    }
}

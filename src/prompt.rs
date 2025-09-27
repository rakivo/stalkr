use crate::util;
use crate::loc::Loc;
use crate::purge::Purges;
use crate::config::Config;
use crate::fm::FileManager;
use crate::mode::ModeValue;
use crate::todo::Description;
use crate::issue::IssueValue;
use crate::tag::InserterValue;

use std::sync::Arc;
use std::fmt::Write;
use std::sync::atomic::{Ordering, AtomicUsize};

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub type ListValue = Prompt;

pub struct Prompt {
    pub mode_value: ModeValue
}

pub enum PrompterTx {
    Issuer(UnboundedSender<IssueValue>),
    Inserter(UnboundedSender<InserterValue>),
    Listing // when listing there's no need to send anything
}

impl PrompterTx {
    #[inline(always)]
    const fn as_issuer_unchecked(&self) -> &UnboundedSender<IssueValue> {
        match self {
            Self::Issuer(i) => i,
            _ => unreachable!()
        }
    }

    #[inline(always)]
    const fn as_inserter_unchecked(&self) -> &UnboundedSender<InserterValue> {
        match self {
            Self::Inserter(i) => i,
            _ => unreachable!()
        }
    }
}

macro_rules! write_buf {
    ($self:expr, $($tt:tt)*) => {
        write!(&mut $self.stdout_buf, $($tt)*)
    }
}

pub struct Prompter {
    pub fm: Arc<FileManager>,
    pub config: Arc<Config>,
    pub tx: PrompterTx,
    // for Listing mode
    pub processed_count: Arc<AtomicUsize>,

    stdout_buf: String
}

impl Prompter {
    const ALL_KEY  : &str = "a";
    const SKIP_KEY : &str = "s";
    const HELP_KEY : &str = "h";

    make_spawn!{
        Prompt,
        #[inline]
        pub fn new(
            fm: Arc<FileManager>,
            config: Arc<Config>,
            tx: PrompterTx,
            processed_count: Arc<AtomicUsize>
        ) -> Self {
            Self {
                fm,
                config,
                tx,
                processed_count,
                stdout_buf: String::with_capacity(1024)
            }
        }
    }

    pub async fn run(&mut self, mut prompter_rx: UnboundedReceiver<Prompt>) {
        let project_url      = self.config.api.get_project_url(&self.config);
        let selection_string = Self::get_selection_string();

        while let Some(prompt) = prompter_rx.recv().await {
            match prompt.mode_value {
                ModeValue::Reporting(mut todos) => {
                    let Some(file_id) = todos.first().map(|t| t.loc.file_id()) else {
                        continue
                    };

                    let file_name = self.fm.get_file_path_unchecked(
                        file_id
                    ).to_owned();

                    let to_report = loop {
                        util::clear_screen();

                        self.print_header(&project_url, &file_name);

                        self.print_todos_with_descriptions(
                            &todos,
                            |todo| &todo.loc,
                            |todo| &todo.title,
                            |todo| todo.description.as_ref()
                        );

                        println!();

                        let cmd = util::ask_input(&selection_string);
                        let cmd = cmd.trim();

                        if cmd.eq_ignore_ascii_case(Self::SKIP_KEY) { break None }
                        if cmd.eq_ignore_ascii_case(Self::HELP_KEY) { Self::print_help(); continue }
                        if cmd.eq_ignore_ascii_case(Self::ALL_KEY)  { break Some(todos) }

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
                                    continue
                                }
                            }
                        }

                        // selection mode
                        let mut report_indexes = Self::get_indexes_from_comma_separated(
                            cmd,
                            todos.len()
                        );

                        if report_indexes.is_empty() { break None }

                        report_indexes.sort_unstable();
                        report_indexes.dedup();

                        let mut to_report = report_indexes.into_iter()
                            .rev()
                            .map(|index| todos.remove(index))
                            .collect::<Vec<_>>();

                        // restore original order
                        to_report.reverse();

                        break Some(to_report)
                    };

                    if let Some(to_report) = to_report {
                        if self.tx
                            .as_issuer_unchecked()
                            .send(ModeValue::Reporting(to_report))
                            .is_err()
                        {
                            eprintln!("[could not send todoʼs to issue worker]");
                        }
                    }
                }

                ModeValue::Listing(todos) => {
                    let Some(file_id) = todos.first().map(|t| t.loc.file_id()) else {
                        continue
                    };

                    util::clear_screen();

                    {
                        let file_name = self.fm.get_file_path_unchecked(file_id);
                        self.print_header(&project_url, &file_name);
                    }

                    self.print_todos_with_descriptions(
                        &todos,
                        |todo| &todo.loc,
                        |todo| &todo.title,
                        |todo| todo.description.as_ref()
                    );

                    self.processed_count.fetch_add(todos.len(), Ordering::SeqCst);

                    println!();
                    Self::print_enter_to("move onto the next file");
                }

                ModeValue::Purging(mut purges) => {
                    let Some(file_id) = purges.first().map(|p| p.tag.todo.loc.file_id()) else {
                        continue
                    };

                    let file_name = self.fm.get_file_path_unchecked(
                        file_id
                    ).to_owned();

                    let to_delete = loop {
                        util::clear_screen();

                        self.print_header(&project_url, &file_name);

                        self.print_todos_with_descriptions(
                            &purges,
                            |purge| &purge.tag.todo.loc,
                            |purge| &purge.tag.todo.title,
                            |purge| purge.tag.todo.description.as_ref()
                        );

                        println!();

                        let cmd = util::ask_input(&selection_string);
                        let cmd = cmd.trim();

                        if cmd.eq_ignore_ascii_case(Self::SKIP_KEY) { break None }
                        if cmd.eq_ignore_ascii_case(Self::HELP_KEY) { Self::print_help(); continue }
                        if cmd.eq_ignore_ascii_case(Self::ALL_KEY)  { break Some(purges) }

                        let mut purge_indexes = Self::get_indexes_from_comma_separated(
                            cmd,
                            purges.len()
                        );

                        if purge_indexes.is_empty() { break None }

                        purge_indexes.sort_unstable();
                        purge_indexes.dedup();

                        let mut selected = purge_indexes
                            .into_iter()
                            .rev()
                            .map(|i| purges.remove(i))
                            .collect::<Vec<_>>();

                        selected.reverse(); // restore original order

                        break Some(Purges { file_id, purges: selected })
                    };

                    if let Some(list) = to_delete {
                        if self.tx
                            .as_inserter_unchecked()
                            .send(InserterValue::Purging(list))
                            .is_err()
                        {
                            eprintln!("[could not send todoʼs to purging worker]");
                        }
                    }
                }
            }
        }
    }

    #[inline]
    fn print_header(&self, project_url: &str, file_name: &str) {
        println!{
            "\
                [{} mode]\
                \n\n\
                [detected project]: {}\
                \n\n\
                [todoʼs from]: {}\
                \n\
            ",
            self.config.mode.to_str_actioning(),
            project_url,
            file_name,
        };
    }

    fn print_todos_with_descriptions<T, FLoc, FTitle, FDesc>(
        &mut self,
        items: &[T],
        get_loc: FLoc,
        get_title: FTitle,
        get_description: FDesc,
    )
    where
        FLoc   : Fn(&T) -> &Loc,
        FTitle : Fn(&T) -> &str,
        FDesc  : Fn(&T) -> Option<&Description>,
    {
        let max_width = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let n_len = (i + 1).to_string().len();
                let line_len = get_loc(item).line_number().to_string().len();
                n_len + 2 + 6 + line_len + 2 // "N. [line X]:"
            }).max().unwrap_or(0);

        for (i, item) in items.iter().enumerate() {
            self.stdout_buf.clear();

            write_buf!(self, "{}. [line {}]:", i + 1, get_loc(item).line_number()).unwrap();

            let pad = max_width.saturating_sub(self.stdout_buf.len()).min(1);
            print!("{}", self.stdout_buf);
            for _ in 0..=pad {
                print!(" ");
            }
            println!("{}", get_title(item));

            if let Some(desc) = get_description(item) {
                println!("   └── description:\n{}", desc.display(9));
            }
        }
    }

    #[inline]
    fn get_selection_string() -> String {
        format!{
            "selection (e.g. 1,2; '{a}' all; '{s}' skip file; '{h}' help; '^C' quit):",
            a = Self::ALL_KEY,
            s = Self::SKIP_KEY,
            h = Self::HELP_KEY,
        }
    }

    #[inline]
    fn get_indexes_from_comma_separated(s: &str, cap: usize) -> Vec<usize> {
        s.split(',').filter_map(|s| {
            s.trim().parse::<usize>().ok().map(|n| n.wrapping_sub(1))
        }).filter(|i| *i < cap).collect::<Vec<_>>()
    }

    #[inline]
    fn print_enter_to(to: &str) {
        _ = util::ask_input(&format!("press <enter> to {to} .."));
    }

    #[inline]
    fn print_help() {
        const HELP_TEXT: &str = r"
HELP:
 - enter comma-separated indices to select todoʼs, e.g. 1,3,5
 - all -> select all
 - s   -> skip this file entirely
 - ^C  -> skip all files
 - h   -> show this help screen
 - prefix with:
     t        -> edit title       (e.g. 6t)
     d        -> edit description (e.g. 6d)
     td or dt -> edit both        (e.g. 4td)
";

        println!("{HELP_TEXT}");
        Self::print_enter_to("continue");
    }
}

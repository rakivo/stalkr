use std::io::{self, Write};

pub fn ask_yn(prompt: &str) -> bool {
    print!("{prompt} [y/n]: ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    let input = input.trim_start();

    input.eq_ignore_ascii_case("y") || input.eq_ignore_ascii_case("yes")
}

pub fn trim_comment_start(s: &str) -> &str {
    s
        .trim_start()
        .trim_start_matches("--")
        .trim_start_matches("//")
        .trim_start_matches('#')
        .trim_start_matches("/*")
        .trim_start()
}

pub fn is_line_a_comment(h_: &str) -> Option<usize> {
    let h = h_.trim_start();

    let first_byte = h.as_bytes().first()?;
    let second_byte = || h.as_bytes().get(1);

    let comment_offset = match first_byte {
        b'#' => 1,
        b'-' if matches!(second_byte()?, b'-') => 2,
        b'/' if matches!(second_byte()?, b'/' | b'*') => 2,
        _ => return None
    };

    Some(h_.len() - h.len() + comment_offset)
}

#[allow(unused)]
pub fn extract_text_from_a_comment(h: &str) -> Option<&str> {
    let comment_end = is_line_a_comment(h)?;
    Some(h[comment_end..].trim())
}

use std::mem;

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum Comment {
    Hash  = b'#',
    Dash  = b'-',
    Slash = b'/',
}

impl Comment {
    #[inline(always)]
    pub const fn from_u8_unchecked(byte: u8) -> Self {
        unsafe { mem::transmute(byte) }
    }

    #[inline]
    pub fn is_line_a_comment(&self, h_: &str) -> Option<usize> {
        let h = h_.trim_start().as_bytes();

        let first_byte = h.first()?;

        let second_byte = || h.get(1);

        let comment_offset = match (*self as _, first_byte) {
            (b'#', b'#') => 1,
            (b'/', b'/') if matches!(second_byte()?, b'/') => 2,
            (b'-', b'-') if matches!(second_byte()?, b'-') => 2,
            _ => return None
        };

        Some(h_.len() - h.len() + comment_offset)
    }
}

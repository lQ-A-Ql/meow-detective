use crate::error::LvmError;

use super::types::{LvSectionRaw, NamedParams, ParsedSection, SegmentRaw};

pub(super) struct Parser<'a> {
    text: &'a str,
    pos: usize,
    line: usize,
}

impl<'a> Parser<'a> {
    pub(super) fn new(text: &'a str) -> Self {
        Self {
            text,
            pos: 0,
            line: 1,
        }
    }

    pub(super) fn find_volume_group(&mut self) -> Result<ParsedSection, LvmError> {
        self.skip_whitespace_and_comments();
        while self.pos < self.text.len() {
            let saved = self.pos;
            let _ = self.expect_ident()?;
            self.skip_whitespace_and_comments();
            match self.current_byte() {
                Some(b'{') => {
                    self.pos = saved;
                    return self.parse_section();
                }
                Some(b'=') => {
                    self.pos = saved;
                    let _ = self.parse_param()?;
                    self.skip_whitespace_and_comments();
                }
                _ => return Err(self.error("expected top-level parameter or volume group")),
            }
        }
        Err(self.error("missing volume group section"))
    }

    fn parse_section(&mut self) -> Result<ParsedSection, LvmError> {
        let name = self.expect_ident()?;
        self.expect_char(b'{')?;
        let mut section = ParsedSection {
            name,
            params: Vec::new(),
            pv_sections: Vec::new(),
            lv_sections: Vec::new(),
        };

        while !self.consume_section_end()? {
            let saved = self.pos;
            let ident = self.expect_ident()?;
            self.skip_whitespace_and_comments();
            match self.current_byte() {
                Some(b'=') => {
                    self.pos = saved;
                    section.params.push(self.parse_param()?);
                }
                Some(b'{') if ident == "physical_volumes" => {
                    section.pv_sections = self.parse_physical_volumes()?;
                }
                Some(b'{') if ident == "logical_volumes" => {
                    section.lv_sections = self.parse_logical_volumes()?;
                }
                Some(b'{') => self.skip_unknown_section()?,
                _ => return Err(self.error("expected parameter or subsection")),
            }
        }
        Ok(section)
    }

    fn parse_physical_volumes(&mut self) -> Result<Vec<NamedParams>, LvmError> {
        self.expect_char(b'{')?;
        let mut physical_volumes = Vec::new();
        while !self.consume_section_end()? {
            let name = self.expect_ident()?;
            self.expect_char(b'{')?;
            let params = self.parse_param_block()?;
            physical_volumes.push((name, params));
        }
        Ok(physical_volumes)
    }

    fn parse_logical_volumes(&mut self) -> Result<Vec<LvSectionRaw>, LvmError> {
        self.expect_char(b'{')?;
        let mut logical_volumes = Vec::new();
        while !self.consume_section_end()? {
            logical_volumes.push(self.parse_logical_volume()?);
        }
        Ok(logical_volumes)
    }

    fn parse_logical_volume(&mut self) -> Result<LvSectionRaw, LvmError> {
        let name = self.expect_ident()?;
        self.expect_char(b'{')?;
        let mut params = Vec::new();
        let mut segments = Vec::new();

        while !self.consume_section_end()? {
            let saved = self.pos;
            let key = self.expect_ident()?;
            self.skip_whitespace_and_comments();
            match self.current_byte() {
                Some(b'=') => {
                    self.pos = saved;
                    params.push(self.parse_param()?);
                }
                Some(b'{') if key.starts_with("segment") => {
                    segments.push(self.parse_segment(key)?);
                }
                _ => return Err(self.error("expected logical-volume parameter or segment")),
            }
        }

        Ok(LvSectionRaw {
            name,
            params,
            segments,
        })
    }

    fn parse_segment(&mut self, name: String) -> Result<SegmentRaw, LvmError> {
        self.expect_char(b'{')?;
        Ok(SegmentRaw {
            name,
            params: self.parse_param_block()?,
        })
    }

    fn parse_param_block(&mut self) -> Result<Vec<(String, String)>, LvmError> {
        let mut params = Vec::new();
        while !self.consume_section_end()? {
            params.push(self.parse_param()?);
        }
        Ok(params)
    }

    fn skip_unknown_section(&mut self) -> Result<(), LvmError> {
        self.expect_char(b'{')?;
        let mut depth = 1u32;
        while self.pos < self.text.len() && depth > 0 {
            match self.text.as_bytes()[self.pos] {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                b'\n' => self.line += 1,
                _ => {}
            }
            self.pos += 1;
        }
        if depth == 0 {
            Ok(())
        } else {
            Err(self.error("unterminated subsection"))
        }
    }

    fn consume_section_end(&mut self) -> Result<bool, LvmError> {
        self.skip_whitespace_and_comments();
        match self.current_byte() {
            Some(b'}') => {
                self.pos += 1;
                Ok(true)
            }
            Some(_) => Ok(false),
            None => Err(self.error("unterminated section")),
        }
    }

    fn parse_param(&mut self) -> Result<(String, String), LvmError> {
        let key = self.expect_ident()?;
        self.expect_char(b'=')?;
        let value = self.parse_value()?;
        Ok((key, value))
    }

    fn parse_value(&mut self) -> Result<String, LvmError> {
        self.skip_whitespace_and_comments();
        match self.current_byte() {
            Some(b'"') => self.parse_quoted_value(),
            Some(b'[') => self.parse_list_value(),
            Some(byte) if byte.is_ascii_digit() || byte == b'-' => self.parse_integer_value(),
            Some(byte) => Err(self.error(&format!("unexpected character '{}'", byte as char))),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn parse_quoted_value(&mut self) -> Result<String, LvmError> {
        self.pos += 1;
        let start = self.pos;
        while let Some(byte) = self.current_byte() {
            if byte == b'"' {
                let value = self.text[start..self.pos].to_string();
                self.pos += 1;
                return Ok(value);
            }
            if byte == b'\n' {
                self.line += 1;
            }
            self.pos += 1;
        }
        Err(self.error("unterminated quoted value"))
    }

    fn parse_list_value(&mut self) -> Result<String, LvmError> {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            match self.current_byte() {
                Some(b']') => {
                    self.pos += 1;
                    return Ok(format!("[{}]", items.join(", ")));
                }
                Some(b',') => self.pos += 1,
                Some(_) => items.push(self.parse_value()?),
                None => return Err(self.error("unterminated list value")),
            }
        }
    }

    fn parse_integer_value(&mut self) -> Result<String, LvmError> {
        let start = self.pos;
        if self.current_byte() == Some(b'-') {
            self.pos += 1;
        }
        let digits_start = self.pos;
        while self
            .current_byte()
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            self.pos += 1;
        }
        if self.pos == digits_start {
            return Err(self.error("expected integer digits"));
        }
        Ok(self.text[start..self.pos].to_string())
    }

    fn expect_ident(&mut self) -> Result<String, LvmError> {
        self.skip_whitespace_and_comments();
        let start = self.pos;
        while self.current_byte().is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'+')
        }) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(self.error("expected identifier"));
        }
        Ok(self.text[start..self.pos].to_string())
    }

    fn expect_char(&mut self, expected: u8) -> Result<(), LvmError> {
        self.skip_whitespace_and_comments();
        if self.current_byte() != Some(expected) {
            return Err(self.error(&format!("expected '{}'", expected as char)));
        }
        self.pos += 1;
        Ok(())
    }

    fn skip_whitespace_and_comments(&mut self) {
        while let Some(byte) = self.current_byte() {
            match byte {
                b' ' | b'\t' | b'\r' => self.pos += 1,
                b'\n' => {
                    self.pos += 1;
                    self.line += 1;
                }
                b'#' => {
                    while self.current_byte().is_some_and(|value| value != b'\n') {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    fn current_byte(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn error(&self, message: &str) -> LvmError {
        LvmError::MetadataParseError {
            line: self.line,
            message: message.to_string(),
        }
    }
}

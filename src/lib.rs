use std::{borrow::Cow, iter::Peekable, path::Path, str::Chars, todo, unimplemented};

use bstr::{BStr, ByteSlice};
use lex_lua::{Keyword, Lexer, Token as LuaToken};

type R<T> = Result<T, Box<dyn std::error::Error>>;


pub enum SingleCommentPart {
    Markdown(String),
    Attr(Attr),
}

#[derive(Debug)]
pub enum Attr {
    Class {
        ty: Type,
        parent_ty: Option<Type>,
        comment: String,
    },
    Type {
        ty: Type,
        comment: String,
    },
    Alias {
        new_name: String,
        old_name: Type,
    },
    Param {
        name: String,
        ty: Type,
        comment: String,
    },
    Return {
        ty: Type,
        comment: String,
    },
    Field {
        vis: Visibility,
        name: String,
        ty: Type,
        comment: String,
    },
    Generic(Vec<Generic>),
    VarArg(Type),
    Lang {
        name: String,
    },
    See(String),
    Unknown(String),
}

impl Attr {
    pub fn try_class(s: &str) -> Option<Self> {
        let s = s.trim_start();
        if s.is_empty() {
            return None;
        }
        let (ty, rem) = if let Some(first_space) = s.find(char::is_whitespace) {
            (
                Type::Single(s[..first_space].to_string()),
                s[first_space..].trim_start(),
            )
        } else {
            (Type::Single(s.to_string()), "")
        };
        let (parent_ty, rem) = if let Some(colon_idx) = rem.find(':') {
            let parent_ty = Type::Single(rem[..colon_idx].trim().to_string());
            (Some(parent_ty), rem[colon_idx..].trim_start())
        } else {
            (None, rem)
        };
        Some(Self::Class {
            ty,
            parent_ty,
            comment: rem.to_string(),
        })
    }

    pub fn try_type(s: &str) -> Option<Self> {
        let mut s = s.trim();
        let mut ret = None;
        while !s.is_empty() {
            let ty_end = s
                .find(|ch: char| ch.is_whitespace() || ch == '|')
                .unwrap_or(s.len());
            let ty_str = &s[..ty_end];
            if ty_end >= s.len() {
                break;
            }
            s = &s[ty_end..];
            if let Some(mut curr) = ret.as_mut() {
                if let Type::Union(list) = &mut curr {
                    list.push(Type::parse(ty_str).expect("Invalid type def"))
                }
            }
        }
        Some(Self::Type {
            ty: ret?,
            comment: s.to_string(),
        })
    }
}

#[derive(Debug)]
pub struct Generic {
    pub name: String,
    pub ty: Option<Type>,
}

#[derive(Debug)]
pub enum Visibility {
    Public,
    Private,
    Protected,
}

#[derive(Debug)]
pub enum Type {
    Single(String),
    Fun {
        args: Vec<(String, Type)>,
        ret: Box<Type>,
    },
    Union(Vec<Type>),
}

struct DocCommentParser<'a> {
    tokenizer: Tokenizer<'a>,
    peek: Option<Token<'a>>,
}

impl<'a> DocCommentParser<'a> {
    pub fn new(s: &'a str) -> Self {
        let mut tokenizer = Tokenizer::new(s);
        let peek = tokenizer.next();
        Self { tokenizer, peek }
    }
    pub fn parse(mut self) -> Option<SingleCommentPart> {
        match self.next_token()? {
            Token::Tag(tag) => {
                let attr = match tag {
                    Tag::Unknown(s) => Attr::Unknown(s.to_string()),
                    Tag::Class => self.class(),
                    Tag::Type => self.type_(),
                    Tag::Alias => self.alias(),
                    Tag::Param => self.param(),
                    Tag::Return => self.return_(),
                    Tag::Field => self.field(),
                    Tag::Generic => self.generic(),
                    Tag::VarArg => self.var_arg(),
                    Tag::Lang => self.lang(),
                    Tag::See => self.see(),
                };
                Some(SingleCommentPart::Attr(attr))
            }
            _ => Some(SingleCommentPart::Markdown(self.tokenizer.orig.to_string())),
        }
    }
    fn next_token(&mut self) -> Option<Token<'a>> {
        let new_next = self.tokenizer.next();
        std::mem::replace(&mut self.peek, new_next)
    }
    fn class(mut self) -> Attr {
        let ty = self.parse_type();
        let parent_ty = if let Some(Token::Punct(Punct::Colon)) = &self.peek {
            self.next_token();
            Some(self.parse_type())
        } else {
            None
        };
        Attr::Class {
            ty,
            parent_ty,
            comment: self.comment(),
        }
    }
    fn type_(&mut self) -> Attr {
        let ty = self.parse_type();
        Attr::Type {
            ty,
            comment: self.comment(),
        }
    }
    fn alias(&mut self) -> Attr {
        let new_name = self.ident();
        let old_name = self.parse_type();
        Attr::Alias { new_name, old_name }
    }
    fn param(&mut self) -> Attr {
        Attr::Param {
            name: self.ident(),
            ty: self.parse_type(),
            comment: self.comment(),
        }
    }
    fn return_(&mut self) -> Attr {
        Attr::Return {
            ty: self.parse_type(),
            comment: self.comment(),
        }
    }
    fn field(&mut self) -> Attr {
        let vis = match self.peek {
            Some(Token::Atom(Atom::Unknown("private"))) => {
                self.next_token();
                Visibility::Private
            }
            Some(Token::Atom(Atom::Unknown("protected"))) => {
                self.next_token();
                Visibility::Protected
            }
            Some(Token::Atom(Atom::Unknown("public"))) => {
                self.next_token();
                Visibility::Public
            }
            _ => Visibility::Public,
        };
        let name = self.ident();
        Attr::Field {
            vis,
            name,
            ty: self.parse_type(),
            comment: self.comment(),
        }
    }
    fn generic(&mut self) -> Attr {
        let mut list = Vec::new();
        loop {
            let name = self.ident();
            let ty = if let Some(Token::Punct(Punct::Colon)) = self.peek {
                let ty = self.parse_type();
                Some(ty)
            } else {
                None
            };
            list.push(Generic { name, ty });
            if let Some(Token::Punct(Punct::Comma)) = self.peek {
                self.next_token();
            } else {
                break;
            }
        }
        Attr::Generic(list)
    }
    fn var_arg(&mut self) -> Attr {
        let ty = self.parse_type();
        Attr::VarArg(ty)
    }
    fn lang(&mut self) -> Attr {
        Attr::Lang {
            name: self.comment(),
        }
    }
    fn see(&mut self) -> Attr {
        Attr::See(self.comment())
    }
    fn parse_type(&mut self) -> Type {
        let mut init = match self.next_token() {
            Some(Token::Atom(Atom::FunStart)) => self.fun_type(),
            Some(Token::Atom(Atom::Unknown(s))) => Type::Single(s.to_string()),
            _ => Type::default(),
        };
        while matches!(self.peek, Some(Token::Punct(Punct::Pipe))) {
            self.next_token();
            let next_ty = match self.next_token() {
                Some(Token::Atom(Atom::FunStart)) => self.fun_type(),
                Some(Token::Atom(Atom::Unknown(s))) => Type::Single(s.to_string()),
                _ => Type::default(),
            };
            match init {
                Type::Single(_) | Type::Fun { .. } => init = Type::Union(vec![init, next_ty]),
                Type::Union(ref mut list) => list.push(next_ty),
            }
        }
        init
    }

    fn fun_type(&mut self) -> Type {
        let mut args = Vec::new();
        while !matches!(self.peek, Some(Token::Punct(Punct::CloseParen))) {
            args.push(self.fun_arg());
            if matches!(self.peek, Some(Token::Punct(Punct::Comma))) {
                self.next_token();
            }
        }
        self.next_token();
        let ret = if matches!(self.peek, Some(Token::Punct(Punct::Colon))) {
            self.next_token();
            self.parse_type()
        } else {
            Type::Single("any".to_string())
        };
        Type::Fun {
            args,
            ret: Box::new(ret),
        }
    }

    fn fun_arg(&mut self) -> (String, Type) {
        let ident = self.ident();
        let ty = if matches!(self.peek, Some(Token::Punct(Punct::Colon))) {
            self.next_token();
            self.parse_type()
        } else {
            Type::default()
        };
        (ident, ty)
    }

    fn ident(&mut self) -> String {
        match self.next_token() {
            Some(Token::Atom(s)) => match s {
                Atom::FunStart => "fun(".to_string(),
                Atom::Unknown(s) => s.to_string(),
            },
            _ => String::new(),
        }
    }
    fn comment(&self) -> String {
        self.tokenizer.orig[self.tokenizer.pos..].trim().to_string()
    }
}

pub enum Token<'a> {
    Tag(Tag<'a>),
    Punct(Punct),
    Atom(Atom<'a>),
}

pub enum Tag<'a> {
    Class,
    Type,
    Alias,
    Param,
    Return,
    Field,
    Generic,
    VarArg,
    Lang,
    See,
    Unknown(&'a str),
}

impl<'a> From<&'a str> for Tag<'a> {
    fn from(s: &'a str) -> Self {
        match s {
            _ => Self::Unknown(s),
        }
    }
}

pub enum Punct {
    Pipe,
    Comma,
    Colon,
    Less,
    Greater,
    CloseParen,
    Array,
}

pub enum Atom<'a> {
    FunStart,
    Unknown(&'a str),
}

pub struct Tokenizer<'a> {
    stream: Peekable<Chars<'a>>,
    orig: &'a str,
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(s: &'a str) -> Self {
        Self {
            stream: s.chars().peekable(),
            orig: s,
            pos: 0,
        }
    }
    pub fn next_item(&mut self) -> Option<Token<'a>> {
        self.skip_whitespace();
        self.check_for_end()?;
        let next = *self.stream.peek()?;
        if next == '@' {
            return self.tag();
        }
        if next.is_alphabetic() || next == '_' {
            return self.atom(self.pos);
        }
        if Self::is_known_punct(next) {
            return self.punct();
        }
        self.atom(self.pos)
    }
    pub fn tag(&mut self) -> Option<Token<'a>> {
        let _at = self.stream.next();
        self.pos += 1;
        let start = self.pos;
        while let Some(&ch) = self.stream.peek() {
            if ch.is_whitespace() {
                break;
            }
            self.pos += 1;
        }
        let tag = self.slice_back(start)?;
        Some(Token::Tag(tag.into()))
    }

    pub fn punct(&mut self) -> Option<Token<'a>> {
        let start = self.pos;
        let next = *self.stream.peek()?;
        let punct = match next {
            '|' => {
                self.skip();
                Punct::Pipe
            }
            ',' => {
                self.skip();
                Punct::Comma
            }
            ':' => {
                self.skip();
                Punct::Colon
            }
            '<' => {
                self.skip();
                Punct::Less
            }
            '>' => {
                self.skip();
                Punct::Greater
            }

            ')' => {
                self.skip();
                Punct::CloseParen
            }
            '[' => {
                self.skip();
                if let Some(&']') = self.stream.peek() {
                    self.skip();
                    Punct::Array
                } else {
                    return self.atom(start);
                }
            }
            _ => return None,
        };
        Some(Token::Punct(punct))
    }

    pub fn atom(&mut self, start: usize) -> Option<Token<'a>> {
        while let Some(&ch) = self.stream.peek() {
            if ch.is_whitespace() {
                break;
            }
            if Self::is_known_punct(ch) {
                break;
            }
            if self.pos - start == 4 && ch == '(' {
                if let Some(last_three) = self.slice_back(self.pos.saturating_sub(3)) {
                    if last_three == "fun" {
                        self.skip();
                        return Some(Token::Atom(Atom::FunStart));
                    }
                }
            }
            self.skip();
        }

        let s = self.slice_back(start)?;
        Some(Token::Atom(Atom::Unknown(s)))
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.stream.peek() {
            if ch.is_whitespace() {
                self.skip();
            }
        }
    }

    fn skip(&mut self) {
        if let Some(ch) = self.stream.next() {
            self.pos += ch.len_utf8();
        }
    }

    fn is_known_punct(ch: char) -> bool {
        match ch {
            '|' | ',' | ':' | '<' | '>' | '[' => true,
            _ => false,
        }
    }

    fn slice_back(&self, start: usize) -> Option<&'a str> {
        self.orig.get(start..self.pos)
    }

    fn check_for_end(&self) -> Option<()> {
        if self.pos >= self.orig.len() {
            None
        } else {
            Some(())
        }
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_item()
    }
}

#[cfg(test)]
mod tests {}

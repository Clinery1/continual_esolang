use anyhow::{
    Result,
    bail,
};
use logos::Logos;
use parser_helper::{
    SimpleError,
    Span,
    LookaheadLexer,
    LogosTokenStream,
    Token as TokenTrait,
    new_parser,
};
use crate::ast::*;

pub use StartOrEnd::*;


#[derive(Debug, Logos, Clone, PartialEq)]
#[logos(skip "[ \t\r\n]")]
pub enum Token<'a> {
    #[regex("[a-zA-Z_][a-zA-Z0-9_]*")]
    Ident(&'a str),

    #[regex("[0-9][0-9_]*")]
    Number(&'a str),

    #[regex("#[a-zA-Z]+", |l|{&l.slice()[1..]})]
    HashLit(&'a str),

    #[regex("\"[^\"]*\"")]
    String(&'a str),

    #[token("/")]
    Slash,

    #[token("(", |_|Start)]
    #[token(")", |_|End)]
    Paren(StartOrEnd),

    #[token("[", |_|Start)]
    #[token("]", |_|End)]
    Square(StartOrEnd),

    EOF,
}
impl<'a> TokenTrait for Token<'a> {
    fn eof()->Self {Self::EOF}
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum StartOrEnd {
    Start,
    End,
}


new_parser!(pub struct Parser<'a, 2, Token<'a>, LogosTokenStream<'a, Token<'a>>>);
// public methods
impl<'a> Parser<'a> {
    pub fn new_from_source(source: &'a str)->Parser<'a> {
        Parser::new(
            Token::lexer(source),
            (),
        )
    }

    pub fn parse(&mut self)->Result<Vec<Expr<'a>>> {
        let mut out = Vec::new();
        while self.peek() != &Token::EOF {
            out.push(self.parse_expr()?);
        }

        return Ok(out);
    }

    pub fn parse_expr(&mut self)->Result<Expr<'a>> {
        match self.peek() {
            Token::Paren(Start)=>match self.peek1() {
                Token::Ident("defCont")=>self.parse_def_cont(),
                Token::Ident("letcc")=>self.parse_letcc(),
                Token::Ident("apply")=>self.parse_apply(),
                Token::Ident("set")=>self.parse_set(),
                // Token::Ident("setf")=>self.parse_setf(),
                Token::Ident("begin")=>self.parse_begin(),
                // Token::Ident("field")=>self.parse_field(),
                Token::Ident("if")=>self.parse_if(),
                _=>self.parse_call(),
            },
            _=>self.parse_lit(),
        }
    }

    fn parse_if(&mut self)->Result<Expr<'a>> {
        self.paren_start()?;
        self.match_ident("if")?;

        let cond = self.parse_expr().map(Box::new)?;
        let expr = self.parse_expr().map(Box::new)?;

        let mut default = None;
        match self.peek() {
            Token::Paren(End)=>self.paren_end()?,
            _=>{
                default = Some(self.parse_expr().map(Box::new)?);
                self.paren_end()?;
            },
        }

        return Ok(Expr::IfElse {cond, expr, default});
    }

    fn parse_field(&mut self)->Result<Expr<'a>> {
        self.paren_start()?;
        self.match_ident("field")?;

        let field = self.ident()?;

        let data = self.parse_expr().map(Box::new)?;

        return Ok(Expr::GetField {field, data});
    }

    fn parse_begin(&mut self)->Result<Expr<'a>> {
        self.paren_start()?;
        self.match_ident("begin")?;

        return self.parse_end_list().map(Expr::Begin);
    }

    fn parse_set(&mut self)->Result<Expr<'a>> {
        self.paren_start()?;
        self.match_ident("set")?;

        let lhs = self.ident()?;

        let data = self.parse_expr().map(Box::new)?;
        self.paren_end()?;

        return Ok(Expr::SetVar(lhs, data));
    }

    fn parse_setf(&mut self)->Result<Expr<'a>> {
        self.paren_start()?;
        self.match_ident("setf")?;

        let lhs = self.parse_expr().map(Box::new)?;
        self.match_token(Token::Slash, "Expected `/`")?;
        let field = self.ident()?;

        let data = self.parse_expr().map(Box::new)?;
        self.paren_end()?;

        return Ok(Expr::SetField {lhs, field, data});
    }

    fn parse_def_cont(&mut self)->Result<Expr<'a>> {
        self.paren_start()?;
        self.match_ident("defCont")?;

        let name = self.ident()?;

        self.match_token(Token::Square(Start), "Expected `[`")?;
        let mut params = Vec::new();
        loop {
            match self.next() {
                Token::Ident(n)=>params.push(n),
                Token::Square(End)=>break,
                t=>bail!("Unexpected token: `{t:?}`"),
            }
        }

        let body = self.parse_end_list()?;

        return Ok(Expr::DefCont {name, params, body});
    }

    fn parse_apply(&mut self)->Result<Expr<'a>> {
        self.paren_start()?;
        self.match_ident("apply")?;

        let lhs = self.parse_expr().map(Box::new)?;

        let args = self.parse_end_list()?;

        return Ok(Expr::Apply{lhs,args});
    }

    fn parse_letcc(&mut self)->Result<Expr<'a>> {
        self.paren_start()?;
        self.match_ident("letcc")?;

        let var = self.ident()?;

        let body = self.parse_expr().map(Box::new)?;
        self.paren_end()?;

        return Ok(Expr::LetCC {var, body});
    }

    fn parse_call(&mut self)->Result<Expr<'a>> {
        self.paren_start()?;
        let to_call = self.parse_expr().map(Box::new)?;

        let args = self.parse_end_list()?;

        return Ok(Expr::Call {to_call, args});
    }

    fn parse_end_list(&mut self)->Result<Vec<Expr<'a>>> {
        let mut out = Vec::new();

        while !self.try_paren_end() {
            out.push(self.parse_expr()?);
        }

        return Ok(out);
    }

    fn parse_lit(&mut self)->Result<Expr<'a>> {
        match self.next() {
            Token::HashLit(lit)=>match lit {
                "t"=>Ok(Expr::Bool(true)),
                "f"=>Ok(Expr::Bool(false)),
                "n"=>Ok(Expr::None),
                _=>bail!("Unknown literal: `{lit}`"),
            },
            Token::String(s)=>{
                let len = s.len();
                Ok(Expr::String(&s[1..len - 1]))
            },
            Token::Number(n)=>{
                let mut out = 0i64;
                let mut mul = 1i64;
                // iterate through the numbers in reverse so we go from lowest->highest
                for c in n.chars().rev() {
                    match c {
                        '0'..='9'=>{
                            let c_num = ((c as u8) - b'0') as i64;
                            let Some(res) = mul.checked_mul(c_num) else {
                                bail!("Integer overflow");
                            };
                            out += res;
                            mul = mul.saturating_mul(10);
                        },
                        _=>{},
                    }
                }

                Ok(Expr::Number(out))
            },
            Token::Ident(name)=>Ok(Expr::GetVar(name)),
            t=>bail!("Unexpected token: {t:?}"),
        }
    }
}
// private helpers
#[allow(unused)]
impl<'a> Parser<'a> {
    #[inline]
    fn match_token<M: Into<String>>(&mut self, tok: Token<'a>, msg: M)->Result<()> {
        self.0.match_token(tok, msg)?;
        return Ok(())
    }

    #[inline]
    fn try_match_token(&mut self, tok: Token<'a>)->bool {
        if self.peek() == &tok {
            self.take_token();
            return true;
        }

        return false;
    }

    #[inline]
    fn peek(&mut self)->&Token<'a> {
        self.lookahead(0)
    }

    fn peek1(&mut self)->&Token<'a> {
        self.lookahead(1)
    }
    
    #[inline]
    fn peek_span(&mut self)->Span {
        self.lookahead_span(0)
    }

    #[inline]
    fn next(&mut self)->Token<'a> {
        self.take_token()
    }

    #[inline]
    fn error(&mut self, msg: impl Into<String>)->SimpleError<String> {
        self.0.error(msg)
    }

    fn ident(&mut self)->Result<&'a str> {
        match self.take_token() {
            Token::Ident(s)=>Ok(s),
            _=>bail!(self.error("Expected identifier")),
        }
    }

    fn match_ident(&mut self, to_match: &str)->Result<()> {
        match self.take_token() {
            Token::Ident(s)=>if s != to_match {
                bail!(self.error(format!("Expected identifier `{}`, but got `{}`", to_match, s)));
            } else {
                Ok(())
            },
            _=>bail!(self.error("Expected identifier")),
        }
    }

    fn paren_start(&mut self)->Result<()> {
        match self.take_token() {
            Token::Paren(Start)=>Ok(()),
            _=>bail!(self.error("Expected `(`")),
        }
    }

    fn paren_end(&mut self)->Result<()> {
        match self.take_token() {
            Token::Paren(End)=>Ok(()),
            _=>bail!(self.error("Expected `)`")),
        }
    }

    fn try_paren_end(&mut self)->bool {
        match self.peek() {
            Token::Paren(End)=>{
                self.take_token();
                true
            },
            _=>false,
        }
    }
}

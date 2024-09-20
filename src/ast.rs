#[derive(Debug, PartialEq)]
pub enum Expr<'a> {
    DefCont {
        name: &'a str,
        params: Vec<&'a str>,
        body: Vec<Self>,
    },

    /// Defines a continuation in `$var` that calls the remainder of the block.
    LetCC {
        var: &'a str,
        body: Box<Self>,
    },
    /// Calls the continuation with the remainder of the block as its first argument.
    Call {
        to_call: Box<Self>,
        args: Vec<Self>,
    },

    Apply {
        lhs: Box<Self>,
        args: Vec<Self>,
    },

    Begin(Vec<Self>),

    IfElse {
        cond: Box<Self>,
        expr: Box<Self>,
        default: Option<Box<Self>>,
    },

    /// Setting a var also defines it if it isn't already defined.
    SetVar(&'a str, Box<Self>),
    GetVar(&'a str),

    SetField {
        lhs: Box<Self>,
        field: &'a str,
        data: Box<Self>,
    },
    GetField {
        data: Box<Self>,
        field: &'a str,
    },

    Number(i64),
    String(&'a str),
    Bool(bool),
    None,
}

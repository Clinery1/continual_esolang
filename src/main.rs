use anyhow::{
    Result,
    bail,
};
use std::{
    fs::read_to_string,
    collections::HashMap,
    mem,
};
use ast::*;


mod parser;
mod ast;


type NativeCont<'a> = fn(&RootScope<'a>, Vec<Data<'a>>)->Result<ContRet<'a>>;


#[derive(Debug, Clone, PartialEq)]
enum Data<'a> {
    Continuation(Continuation<'a>),
    String(String),
    Number(i64),
    Bool(bool),
    None,
}

#[derive(Debug, Clone, PartialEq)]
enum ExprCont<'a> {
    Single(&'a Expr<'a>),
    Multiple(&'a [Expr<'a>]),
    None,
}

enum ContRet<'a> {
    Apply(Continuation<'a>, Vec<Data<'a>>),
    Data(Continuation<'a>, Data<'a>),
}

#[derive(Debug, Clone, PartialEq)]
enum Continuation<'a> {
    Native(NativeCont<'a>),
    Return,
    Normal {
        scopes: Vec<HashMap<&'a str, Data<'a>>>,
        vars: HashMap<&'a str, Data<'a>>,
        exprs: ExprCont<'a>,
    },
    Function {
        params: &'a [&'a str],
        body: &'a [Expr<'a>],
    },
}
impl<'a> Continuation<'a> {
    pub fn run(mut self, root: &RootScope<'a>, mut args: Vec<Data<'a>>)->Result<Data<'a>> {
        match self {
            Self::Function{params,body}=>{
                if args.len() != params.len() {
                    bail!("Expected {} args for function, but got {}", params.len(), args.len());
                }

                self = Self::Normal {
                    scopes: Vec::new(),
                    vars: params.into_iter()
                        .map(|s|*s)
                        .zip(args.into_iter())
                        .collect(),
                    exprs: ExprCont::Multiple(body),
                };
                args = Vec::new();
            },
            Self::Native(f)=>match f(root, args)? {
                ContRet::Apply(cont,d)=>return cont.run(root, d),
                ContRet::Data(_, d)=>return Ok(d),
            },
            _=>{},
        }

        return self.run_inner(root, args);
    }

    /// Should not be called outside of `Self`
    fn run_inner(mut self, root: &RootScope<'a>, args: Vec<Data<'a>>)->Result<Data<'a>> {
        match &self {
            Self::Return=>return Ok(args.get(0).cloned().unwrap_or(Data::None)),
            Self::Normal{exprs,..}=>{
                match exprs {
                    ExprCont::Single(expr)=>{
                        let expr = *expr;
                        match self.run_single(root, expr)? {
                            ContRet::Apply(cont, d)=>return cont.run(root, d),
                            ContRet::Data(_, data)=>return Ok(data),
                        }
                    },
                    ExprCont::Multiple(exprs)=>{
                        let mut data = Data::None;
                        for expr in *exprs {
                            match self.run_single(root, expr)? {
                                ContRet::Apply(cont, d)=>return cont.run(root, d),
                                ContRet::Data(s, d)=>{
                                    data = d;
                                    self = s;
                                },
                            }
                        }

                        return Ok(data);
                    },
                    ExprCont::None=>return Ok(Data::None),
                }
            },
            _=>unreachable!(),
        }
    }

    fn run_single(mut self, root: &RootScope<'a>, expr: &'a Expr<'a>)->Result<ContRet<'a>> {
        let Self::Normal{scopes,vars,..} = &mut self else {panic!("Can only be called from a `Continuation::Normal` instance!")};
        match expr {
            Expr::DefCont{name,params,body}=>{
                vars.insert(name, Data::Continuation(Continuation::Function{params,body}));

                return Ok(ContRet::Data(self, Data::None));
            },
            Expr::Begin(exprs)=>{
                let mut ret = Data::None;
                for expr in exprs {
                    match self.run_single(root, expr)? {
                        ContRet::Apply(c,d)=>return Ok(ContRet::Apply(c, d)),
                        ContRet::Data(s,d)=>{
                            ret = d;
                            self = s;
                        },
                    }
                }

                return Ok(ContRet::Data(self, ret));
            },
            Expr::LetCC{var,body}=>{
                scopes.push(mem::take(vars));
                vars.insert(var, Data::Continuation(Continuation::Return));
                match self.run_single(root, body)? {
                    ContRet::Apply(s,d)=>return Ok(ContRet::Apply(s, d)),
                    ContRet::Data(s,d)=>{
                        self = s;

                        let Self::Normal{scopes,vars,..} = &mut self else {unreachable!()};
                        let scope = scopes.pop().unwrap();
                        *vars = scope;

                        return Ok(ContRet::Data(self, d));
                    },
                }
            },
            Expr::Call{to_call,args}=>{
                let lhs_data = match self.run_single(root, to_call)? {
                    ContRet::Apply(s,d)=>return Ok(ContRet::Apply(s, d)),
                    ContRet::Data(s,d)=>{
                        self = s;
                        d
                    },
                };

                let mut data_args = vec![Data::Continuation(Continuation::Return)];
                for arg in args {
                    match self.run_single(root, arg)? {
                        ContRet::Apply(s,d)=>return Ok(ContRet::Apply(s, d)),
                        ContRet::Data(s,d)=>{
                            data_args.push(d);
                            self = s;
                        },
                    }
                }
                
                match lhs_data {
                    Data::Continuation(cont)=>{
                        let data = cont.run(root, data_args)?;
                        return Ok(ContRet::Data(self, data));
                    },
                    _=>bail!("Apply LHS is not a continuation"),
                }
            },
            Expr::Apply{lhs,args}=>{
                let lhs_data = match self.run_single(root, lhs)? {
                    ContRet::Apply(s,d)=>return Ok(ContRet::Apply(s, d)),
                    ContRet::Data(s,d)=>{
                        self = s;
                        d
                    },
                };

                let mut data_args = Vec::new();
                for arg in args {
                    match self.run_single(root, arg)? {
                        ContRet::Apply(s,d)=>return Ok(ContRet::Apply(s, d)),
                        ContRet::Data(s,d)=>{
                            data_args.push(d);
                            self = s;
                        },
                    }
                }
                
                match lhs_data {
                    Data::Continuation(cont)=>return Ok(ContRet::Apply(cont, data_args)),
                    _=>bail!("Apply LHS is not a continuation"),
                }
            },
            Expr::IfElse{cond,expr,default}=>{
                let mut res = false;
                match self.run_single(root, cond)? {
                    ContRet::Apply(s,d)=>return Ok(ContRet::Apply(s, d)),
                    ContRet::Data(s,d)=>{
                        match d {
                            Data::Bool(b)=>res = b,
                            _=>{},
                        }
                        self = s;
                    },
                }

                if res {
                    return self.run_single(root, expr);
                } else if let Some(def) = default {
                    return self.run_single(root, def);
                } else {
                    return Ok(ContRet::Data(self, Data::None));
                }
            },
            Expr::SetVar(name,data)=>match self.run_single(root, data)? {
                ContRet::Apply(s,d)=>return Ok(ContRet::Apply(s, d)),
                ContRet::Data(s,d)=>{
                    self = s;
                    let Self::Normal{vars,..} = &mut self else {unreachable!()};
                    vars.insert(name, d);
                    return Ok(ContRet::Data(self, Data::None));
                },
            },
            Expr::GetVar(name)=>{
                if let Some(data) = vars.get(name) {
                    let data = data.clone();
                    return Ok(ContRet::Data(self, data));
                }
                for scope in scopes.iter().rev() {
                    if let Some(data) = scope.get(name) {
                        let data = data.clone();
                        return Ok(ContRet::Data(self, data));
                    }
                }
                if let Some(cont) = root.get(name) {
                    return Ok(ContRet::Data(self, Data::Continuation(cont)));
                }

                bail!("No variable with the name `{name}`");
            },
            // Expr::SetField{lhs,field,data}=>{
            //     todo!();
            // },
            // Expr::GetField{data,field}=>{
            //     todo!();
            // },
            Expr::String(s)=>Ok(ContRet::Data(self, Data::String(s.to_string()))),
            Expr::Number(n)=>Ok(ContRet::Data(self, Data::Number(*n))),
            Expr::Bool(b)=>Ok(ContRet::Data(self, Data::Bool(*b))),
            Expr::None=>Ok(ContRet::Data(self, Data::None)),
            _=>todo!(),
        }
    }
}


struct RootScope<'a>(HashMap<&'a str, Continuation<'a>>);
impl<'a> RootScope<'a> {
    pub fn new(exprs: &'a [Expr<'a>])->Self {
        let mut map = HashMap::new();
        for expr in exprs {
            match expr {
                Expr::DefCont{name,params,body}=>{
                    let cont = Continuation::Function {
                        body: body.as_slice(),
                        params,
                    };
                    map.insert(*name, cont);
                },
                _=>{},
            }
        }

        return RootScope(map);
    }

    pub fn run_cont(&self, name: &'a str, args: Vec<Data<'a>>)->Result<Data<'a>> {
        if let Some(cont) = self.0.get(name) {
            return cont.clone().run(self, args);
        }

        bail!("No continuation named `{name}`");
    }

    pub fn get(&self, name: &'a str)->Option<Continuation<'a>> {
        self.0.get(name).cloned()
    }

    pub fn add_native(&mut self, name: &'a str, f: NativeCont<'a>) {
        self.0.insert(name, Continuation::Native(f));
    }
}


fn main() {
    let source = read_to_string("example.cont").unwrap();
    let mut parser = parser::Parser::new_from_source(&source);
    match parser.parse() {
        Ok(res)=>{
            dbg!(&res);
            let mut root = RootScope::new(&res);
            root.add_native("println", println_native);

            root.add_native("add", add);
            root.add_native("sub", sub);
            root.add_native("mul", mul);
            root.add_native("rem", rem);

            root.add_native("eq", eq);
            root.add_native("and", and);
            dbg!(root.run_cont("main", vec![])).ok();
        },
        Err(err)=>{
            eprintln!("{err}");
        },
    }
}

fn eq<'a>(_: &RootScope<'a>, args: Vec<Data<'a>>)->Result<ContRet<'a>> {
    if args.len() == 0 {
        bail!("Expected continuation for first argument, but got no args");
    }
    let mut args_iter = args.into_iter();
    let cont = args_iter.next().unwrap();
    let Some(first) = args_iter.next() else {return ret_cont_data(cont, vec![Data::Bool(true)])};

    let mut ret = true;
    for arg in args_iter {
        if first != arg {
            ret = false;
            break;
        }
    }

    return ret_cont_data(cont, vec![Data::Bool(ret)]);
}

fn and<'a>(_: &RootScope<'a>, args: Vec<Data<'a>>)->Result<ContRet<'a>> {
    if args.len() == 0 {
        bail!("Expected continuation for first argument, but got no args");
    }
    let mut args_iter = args.into_iter();
    let cont = args_iter.next().unwrap();

    let mut ret = true;
    for arg in args_iter {
        match arg {
            Data::Bool(true)=>{},
            _=>{
                ret = false;
                break;
            },
        }
    }

    return ret_cont_data(cont, vec![Data::Bool(ret)]);
}

fn add<'a>(_: &RootScope<'a>, args: Vec<Data<'a>>)->Result<ContRet<'a>> {
    if args.len() == 0 {
        bail!("Expected continuation for first argument, but got no args");
    }
    let mut args_iter = args.into_iter();
    let cont = args_iter.next().unwrap();

    let mut total = 0;
    for arg in args_iter {
        match arg {
            Data::Number(n)=>total += n,
            _=>{},
        }
    }

    return ret_cont_data(cont, vec![Data::Number(total)]);
}

fn sub<'a>(_: &RootScope<'a>, args: Vec<Data<'a>>)->Result<ContRet<'a>> {
    if args.len() == 0 {
        bail!("Expected continuation for first argument, but got no args");
    }
    let mut args_iter = args.into_iter();
    let cont = args_iter.next().unwrap();

    let mut total = 0;
    let mut first = true;
    for arg in args_iter {
        match arg {
            Data::Number(n)=>if first {
                total = n;
                first = false;
            } else {
                total -= n;
            },
            _=>{},
        }
    }

    return ret_cont_data(cont, vec![Data::Number(total)]);
}

fn mul<'a>(_: &RootScope<'a>, args: Vec<Data<'a>>)->Result<ContRet<'a>> {
    if args.len() == 0 {
        bail!("Expected continuation for first argument, but got no args");
    }
    let mut args_iter = args.into_iter();
    let cont = args_iter.next().unwrap();

    let mut total = 0;
    let mut first = true;
    for arg in args_iter {
        match arg {
            Data::Number(n)=>if first {
                total = n;
                first = false;
            } else {
                total *= n;
            },
            _=>{},
        }
    }

    return ret_cont_data(cont, vec![Data::Number(total)]);
}

fn rem<'a>(_: &RootScope<'a>, args: Vec<Data<'a>>)->Result<ContRet<'a>> {
    if args.len() == 0 {
        bail!("Expected continuation for first argument, but got no args");
    }
    let mut args_iter = args.into_iter();
    let cont = args_iter.next().unwrap();

    let mut rem = 0;
    let mut first = true;
    for arg in args_iter {
        match arg {
            Data::Number(n)=>if first {
                rem = n;
                first = false;
            } else {
                rem %= n;
            },
            _=>{},
        }
    }

    return ret_cont_data(cont, vec![Data::Number(rem)]);
}

fn println_native<'a>(_: &RootScope<'a>, args: Vec<Data<'a>>)->Result<ContRet<'a>> {
    if args.len() < 2 {
        bail!("Expected 2 args for println");
    }
    let mut args_iter = args.into_iter();

    let cont = args_iter.next().unwrap();
    for msg in args_iter {
        match msg {
            Data::String(s)=>print!("{s}"),
            Data::Number(n)=>print!("{n}"),
            Data::Bool(true)=>print!("#t"),
            Data::Bool(false)=>print!("#f"),
            Data::None=>print!("#n"),
            Data::Continuation(_)=>print!("<cont>"),
        }
    }
    println!();

    return ret_cont_data(cont, vec![Data::None]);
}

fn ret_cont_data<'a>(cont: Data<'a>, args: Vec<Data<'a>>)->Result<ContRet<'a>> {
    match cont {
        Data::Continuation(cont)=>return Ok(ContRet::Apply(cont, args)),
        _=>bail!("Expected continuation for first argument"),
    }
}

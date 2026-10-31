//! Derived fields — Fieldplot name for VisIt-style expressions.
//!
//! Expression grammar (first slice): numbers, dataset variable names,
//! `+ - * /`, parentheses, and `sqrt|abs|hypot|mag|min|max`.

use serde::Serialize;

use crate::dataset::{Dataset, Location, VariableData};
use crate::error::{Error, Result};
use crate::field::find_var;

/// One user-defined derived field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DerivedField {
    pub name: String,
    pub expression: String,
}

/// Evaluate `expr` at one sample given named variable values.
pub fn eval_expression(expr: &str, vars: &[(String, f64)]) -> Result<f64> {
    let tokens = tokenize(expr)?;
    let mut p = Parser {
        tokens: &tokens,
        pos: 0,
        vars,
    };
    let v = p.parse_expr()?;
    if p.pos != p.tokens.len() {
        return Err(Error::plot(format!(
            "derived expression: unexpected token near '{}'",
            expr
        )));
    }
    Ok(v)
}

/// Append a derived scalar to every zone (nodal, matching first field length).
pub fn apply_derived(ds: &Dataset, name: &str, expression: &str) -> Result<Dataset> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::plot("derived field name is empty"));
    }
    if ds.variables.iter().any(|v| v.eq_ignore_ascii_case(name)) {
        return Err(Error::plot(format!(
            "derived field '{name}' already exists in the dataset"
        )));
    }
    // Validate expression against zone 0 (or empty vars) before mutating.
    let probe_vars: Vec<(String, f64)> = ds
        .variables
        .iter()
        .map(|n| (n.clone(), 0.0))
        .collect();
    let _ = eval_expression(expression, &probe_vars)?;

    let mut out = ds.clone();
    out.variables.push(name.to_string());
    for zone in &mut out.zones {
        let n = zone
            .fields
            .first()
            .map(|f| f.values.len())
            .unwrap_or(zone.node_count());
        let loc = zone
            .fields
            .first()
            .map(|f| f.location)
            .unwrap_or(Location::Nodal);
        let mut values = Vec::with_capacity(n);
        for i in 0..n {
            let mut pairs = Vec::with_capacity(ds.variables.len());
            for (vi, vname) in ds.variables.iter().enumerate() {
                let val = zone
                    .fields
                    .get(vi)
                    .and_then(|f| f.values.get(i).copied())
                    .unwrap_or(f64::NAN);
                pairs.push((vname.clone(), val));
            }
            values.push(eval_expression(expression, &pairs)?);
        }
        zone.fields.push(VariableData {
            location: loc,
            values,
        });
    }
    Ok(out)
}

/// Write a minimal PLT ASCII dump (POINT packing) for ordered IJ/IJK zones.
pub fn write_plt_ascii(ds: &Dataset, path: &std::path::Path) -> Result<()> {
    use std::fmt::Write as _;
    let mut buf = String::new();
    let title = ds.title.as_deref().unwrap_or("Fieldplot derived");
    writeln!(buf, "TITLE = \"{}\"", title.replace('"', "'")).unwrap();
    write!(buf, "VARIABLES =").unwrap();
    for (i, v) in ds.variables.iter().enumerate() {
        if i > 0 {
            write!(buf, ",").unwrap();
        }
        write!(buf, " \"{}\"", v.replace('"', "'")).unwrap();
    }
    writeln!(buf).unwrap();

    for (zi, zone) in ds.zones.iter().enumerate() {
        let meta = ds.meta(zi);
        match &zone.topology {
            crate::dataset::Topology::Ordered(dims) => {
                let (i, j, k) = match *dims {
                    crate::dataset::OrderedDims::I { i } => (i, 1usize, 1usize),
                    crate::dataset::OrderedDims::Ij { i, j } => (i, j, 1usize),
                    crate::dataset::OrderedDims::Ijk { i, j, k } => (i, j, k),
                };
                write!(
                    buf,
                    "ZONE T=\"{}\", I={i}, J={j}",
                    zone.name.replace('"', "'")
                )
                .unwrap();
                if k > 1 {
                    write!(buf, ", K={k}").unwrap();
                }
                if let Some(t) = meta.solution_time {
                    write!(buf, ", SOLUTIONTIME={t}").unwrap();
                }
                if let Some(s) = meta.strand_id {
                    write!(buf, ", STRANDID={s}").unwrap();
                }
                writeln!(buf, ", DATAPACKING=POINT").unwrap();
                let n = zone.node_count();
                for row in 0..n {
                    for (fi, _) in ds.variables.iter().enumerate() {
                        let v = zone
                            .fields
                            .get(fi)
                            .and_then(|f| f.values.get(row).copied())
                            .unwrap_or(0.0);
                        if fi > 0 {
                            write!(buf, " ").unwrap();
                        }
                        write!(buf, "{v}").unwrap();
                    }
                    writeln!(buf).unwrap();
                }
            }
            crate::dataset::Topology::FiniteElement { .. } => {
                return Err(Error::plot(
                    "derived write: FE zones not supported in this PLT ASCII dump",
                ));
            }
        }
    }
    std::fs::write(path, buf).map_err(|e| Error::io(path, e))?;
    Ok(())
}

/// Min/max of a variable after derive (for JSON summaries).
pub fn field_minmax(ds: &Dataset, name: &str) -> Result<(f64, f64)> {
    let idx = find_var(ds, name)?;
    let mut mn = f64::INFINITY;
    let mut mx = f64::NEG_INFINITY;
    for zone in &ds.zones {
        if let Some(f) = zone.fields.get(idx) {
            for &v in &f.values {
                if v.is_finite() {
                    mn = mn.min(v);
                    mx = mx.max(v);
                }
            }
        }
    }
    if !mn.is_finite() {
        return Err(Error::plot(format!("derived field '{name}' has no finite samples")));
    }
    Ok((mn, mx))
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(char),
    LParen,
    RParen,
    Comma,
}

fn tokenize(expr: &str) -> Result<Vec<Tok>> {
    let mut out = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == 'e' || chars[i] == 'E' || chars[i] == '+' || chars[i] == '-') {
                // stop signed exponent carefully
                if (chars[i] == '+' || chars[i] == '-')
                    && !(chars[i - 1] == 'e' || chars[i - 1] == 'E')
                {
                    break;
                }
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            let n: f64 = s
                .parse()
                .map_err(|_| Error::plot(format!("derived expression: bad number '{s}'")))?;
            out.push(Tok::Num(n));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            out.push(Tok::Ident(s));
            continue;
        }
        match c {
            '+' | '-' | '*' | '/' => {
                out.push(Tok::Op(c));
                i += 1;
            }
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            other => {
                return Err(Error::plot(format!(
                    "derived expression: unexpected character '{other}'"
                )));
            }
        }
    }
    Ok(out)
}

struct Parser<'a> {
    tokens: &'a [Tok],
    pos: usize,
    vars: &'a [(String, f64)],
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a Tok> {
        self.tokens.get(self.pos)
    }

    fn bump(&mut self) -> Option<&'a Tok> {
        let t = self.tokens.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn parse_expr(&mut self) -> Result<f64> {
        let mut v = self.parse_term()?;
        while let Some(Tok::Op(op @ ('+' | '-'))) = self.peek().cloned() {
            self.bump();
            let r = self.parse_term()?;
            v = if op == '+' { v + r } else { v - r };
        }
        Ok(v)
    }

    fn parse_term(&mut self) -> Result<f64> {
        let mut v = self.parse_unary()?;
        while let Some(Tok::Op(op @ ('*' | '/'))) = self.peek().cloned() {
            self.bump();
            let r = self.parse_unary()?;
            v = if op == '*' {
                v * r
            } else if r == 0.0 {
                f64::NAN
            } else {
                v / r
            };
        }
        Ok(v)
    }

    fn parse_unary(&mut self) -> Result<f64> {
        if let Some(Tok::Op('-')) = self.peek() {
            self.bump();
            return Ok(-self.parse_unary()?);
        }
        if let Some(Tok::Op('+')) = self.peek() {
            self.bump();
            return self.parse_unary();
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<f64> {
        match self.bump().cloned() {
            Some(Tok::Num(n)) => Ok(n),
            Some(Tok::Ident(name)) => {
                if let Some(Tok::LParen) = self.peek() {
                    self.bump();
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Some(Tok::RParen)) {
                        args.push(self.parse_expr()?);
                        while let Some(Tok::Comma) = self.peek() {
                            self.bump();
                            args.push(self.parse_expr()?);
                        }
                    }
                    match self.bump() {
                        Some(Tok::RParen) => {}
                        _ => {
                            return Err(Error::plot(format!(
                                "derived expression: expected ')' after {name}("
                            )))
                        }
                    }
                    call_fn(&name, &args)
                } else {
                    lookup_var(self.vars, &name)
                }
            }
            Some(Tok::LParen) => {
                let v = self.parse_expr()?;
                match self.bump() {
                    Some(Tok::RParen) => Ok(v),
                    _ => Err(Error::plot("derived expression: unmatched '('")),
                }
            }
            other => Err(Error::plot(format!(
                "derived expression: unexpected token {other:?}"
            ))),
        }
    }
}

fn lookup_var(vars: &[(String, f64)], name: &str) -> Result<f64> {
    vars.iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| *v)
        .ok_or_else(|| {
            Error::plot(format!(
                "derived expression: unknown variable '{name}'"
            ))
        })
}

fn call_fn(name: &str, args: &[f64]) -> Result<f64> {
    let n = name.to_ascii_lowercase();
    match n.as_str() {
        "sqrt" => {
            one(args, "sqrt")?;
            Ok(args[0].sqrt())
        }
        "abs" => {
            one(args, "abs")?;
            Ok(args[0].abs())
        }
        "hypot" | "mag" => {
            if args.len() == 2 {
                Ok(args[0].hypot(args[1]))
            } else if args.len() == 3 {
                Ok((args[0] * args[0] + args[1] * args[1] + args[2] * args[2]).sqrt())
            } else {
                Err(Error::plot(format!(
                    "{name}() expects 2 or 3 arguments"
                )))
            }
        }
        "min" => {
            two(args, "min")?;
            Ok(args[0].min(args[1]))
        }
        "max" => {
            two(args, "max")?;
            Ok(args[0].max(args[1]))
        }
        other => Err(Error::plot(format!(
            "derived expression: unknown function '{other}'"
        ))),
    }
}

fn one(args: &[f64], name: &str) -> Result<()> {
    if args.len() == 1 {
        Ok(())
    } else {
        Err(Error::plot(format!("{name}() expects 1 argument")))
    }
}

fn two(args: &[f64], name: &str) -> Result<()> {
    if args.len() == 2 {
        Ok(())
    } else {
        Err(Error::plot(format!("{name}() expects 2 arguments")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::load_path;
    use std::path::PathBuf;

    #[test]
    fn eval_arith_and_hypot() {
        let vars = vec![("U".into(), 3.0), ("V".into(), 4.0), ("P".into(), 2.0)];
        assert!((eval_expression("hypot(U,V)", &vars).unwrap() - 5.0).abs() < 1e-12);
        assert!((eval_expression("P*2 + 1", &vars).unwrap() - 5.0).abs() < 1e-12);
        assert!((eval_expression("sqrt(U*U + V*V)", &vars).unwrap() - 5.0).abs() < 1e-12);
    }

    #[test]
    fn apply_derived_on_flow_fixture() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flow.dat");
        let ds = load_path(&path).unwrap();
        let out = apply_derived(&ds, "Speed", "hypot(U,V)").unwrap();
        assert!(out.variables.iter().any(|v| v == "Speed"));
        let (mn, mx) = field_minmax(&out, "Speed").unwrap();
        assert!(mn.is_finite() && mx.is_finite());
        assert!(mn > 0.0);
    }
}

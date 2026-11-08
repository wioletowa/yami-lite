// Калькулятор лаунчера: выражение в поиске («2+2*3», «=sqrt(2)») считается на лету, Enter копирует ответ. Свой разбор рекурсивным спуском:
// + - * / % ^, скобки, унарный минус, десятичная точка или запятая, 0x/0b, константы pi и e,
// функции sqrt abs round floor ceil ln log sin cos tan. Ни потоков, ни crate'ов.

/// выражение ли это вообще: «=» в начале или цифры с хотя бы одним оператором. Иначе обычный
/// поиск программ («7zip», «2048») не превращается в калькулятор.
pub fn looks_like(q: &str) -> bool {
    let q = q.trim();
    if let Some(r) = q.strip_prefix('=') {
        return !r.trim().is_empty();
    }
    let has_digit = q.chars().any(|c| c.is_ascii_digit());
    let has_op = q.chars().skip(1).any(|c| "+-*/%^×÷".contains(c)) || q.contains('(');
    let only = q.chars().all(|c| c.is_ascii_alphanumeric() || " .,+-*/%^()×÷_".contains(c));
    has_digit && has_op && only
}

/// посчитать; None — не выражение или ошибка (деление на ноль, лишняя скобка)
pub fn eval(q: &str) -> Option<f64> {
    let q = q.trim().trim_start_matches('=');
    let src: Vec<char> = q.chars().map(|c| match c {
        '×' => '*',
        '÷' => '/',
        ',' => '.',
        c => c.to_ascii_lowercase(),
    }).filter(|c| !c.is_whitespace()).collect();
    if src.is_empty() || src.len() > 256 {
        return None;
    }
    let mut p = P { s: &src, i: 0, depth: 0 };
    let v = p.sum()?;
    (p.i == src.len() && v.is_finite()).then_some(v)
}

/// ответ для строки: до 12 значащих цифр, без хвостовых нулей и «-0»
pub fn format(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if v.abs() >= 1e15 || v.abs() < 1e-9 {
        return format!("{v:e}");
    }
    let digits = (12 - v.abs().log10().floor() as i32 - 1).clamp(0, 12) as usize;
    let s = format!("{v:.digits$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

struct P<'a> {
    s: &'a [char],
    i: usize,
    /// вложенность скобок и функций: «((((…» в тысячу уровней не уронит стек
    depth: u32,
}

impl P<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn sum(&mut self) -> Option<f64> {
        let mut v = self.product()?;
        loop {
            if self.eat('+') {
                v += self.product()?;
            } else if self.eat('-') {
                v -= self.product()?;
            } else {
                return Some(v);
            }
        }
    }
    fn product(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        loop {
            if self.eat('*') {
                v *= self.unary()?;
            } else if self.eat('/') {
                let d = self.unary()?;
                if d == 0.0 {
                    return None;
                }
                v /= d;
            } else if self.eat('%') {
                let d = self.unary()?;
                if d == 0.0 {
                    return None;
                }
                v %= d;
            } else {
                return Some(v);
            }
        }
    }
    /// минус слабее степени: -3^2 = -9
    fn unary(&mut self) -> Option<f64> {
        if self.eat('-') {
            return Some(-self.unary()?);
        }
        if self.eat('+') {
            return self.unary();
        }
        self.power()
    }
    /// степень правоассоциативна: 2^3^2 = 2^9; показатель может быть отрицательным
    fn power(&mut self) -> Option<f64> {
        let b = self.atom()?;
        if self.eat('^') {
            let e = self.unary()?;
            return Some(b.powf(e));
        }
        Some(b)
    }
    fn atom(&mut self) -> Option<f64> {
        self.depth += 1;
        if self.depth > 64 {
            return None;
        }
        let r = self.atom_inner();
        self.depth -= 1;
        r
    }
    fn atom_inner(&mut self) -> Option<f64> {
        if self.eat('(') {
            let v = self.sum()?;
            return self.eat(')').then_some(v);
        }
        let c = self.peek()?;
        if c.is_ascii_digit() || c == '.' {
            return self.number();
        }
        if c.is_ascii_alphabetic() {
            let start = self.i;
            while self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) {
                self.i += 1;
            }
            let name: String = self.s[start..self.i].iter().collect();
            return match name.as_str() {
                "pi" => Some(std::f64::consts::PI),
                "e" => Some(std::f64::consts::E),
                _ => {
                    let f: fn(f64) -> f64 = match name.as_str() {
                        "sqrt" => f64::sqrt,
                        "abs" => f64::abs,
                        "round" => f64::round,
                        "floor" => f64::floor,
                        "ceil" => f64::ceil,
                        "ln" => f64::ln,
                        "log" => f64::log10,
                        "sin" => f64::sin,
                        "cos" => f64::cos,
                        "tan" => f64::tan,
                        _ => return None,
                    };
                    let a = self.atom()?;
                    Some(f(a))
                }
            };
        }
        None
    }
    fn number(&mut self) -> Option<f64> {
        // 0x1f, 0b101
        if self.peek() == Some('0') && matches!(self.s.get(self.i + 1), Some('x') | Some('b')) {
            let radix = if self.s[self.i + 1] == 'x' { 16 } else { 2 };
            self.i += 2;
            let start = self.i;
            while self.peek().is_some_and(|c| c.is_digit(radix)) {
                self.i += 1;
            }
            let t: String = self.s[start..self.i].iter().collect();
            return u64::from_str_radix(&t, radix).ok().map(|v| v as f64);
        }
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            self.i += 1;
        }
        let t: String = self.s[start..self.i].iter().collect();
        t.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arithmetic() {
        assert_eq!(eval("2+2*3"), Some(8.0));
        assert_eq!(eval("(2+2)*3"), Some(12.0));
        assert_eq!(eval("2^3^2"), Some(512.0));
        assert_eq!(eval("-3^2"), Some(-9.0));
        assert_eq!(eval("2^-1"), Some(0.5));
        assert_eq!(eval("10 % 4"), Some(2.0));
        assert_eq!(eval("1,5*2"), Some(3.0));
        assert_eq!(eval("0x10+0b11"), Some(19.0));
        assert_eq!(eval("=sqrt(16)"), Some(4.0));
        assert_eq!(eval("6×7"), Some(42.0));
        assert_eq!(eval("1/0"), None);
        assert_eq!(eval("(1+2"), None);
        assert_eq!(eval("foo(2)"), None);
        assert_eq!(eval(&"(".repeat(500)), None);
    }
    #[test]
    fn detect() {
        assert!(looks_like("2+2"));
        assert!(looks_like("= pi"));
        assert!(!looks_like("7zip"));
        assert!(!looks_like("2048"));
        assert!(!looks_like("firefox"));
        assert!(!looks_like("-5"));
    }
    #[test]
    fn fmt() {
        assert_eq!(format(0.1 + 0.2), "0.3");
        assert_eq!(format(1.0 / 3.0), "0.333333333333");
        assert_eq!(format(42.0), "42");
        assert_eq!(format(-0.0), "0");
    }
}

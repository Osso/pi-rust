// Copyright 2011 the V8 project authors. All rights reserved.
// Rust adaptation of Node v26.11.1 deps/v8/src/date/dateparser{,-inl}.{h,cc}.
// BSD-3-Clause license: see ../../../docs/porting/vendor/V8-LICENSE.
//! V8's UTF-16 date scanner and ISO/legacy composers, without runtime telemetry.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Number,
    Symbol,
    Space,
    Unknown,
    End,
    Invalid,
    Month,
    Zone,
    Separator,
    AmPm,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: Kind,
    value: i64,
    length: usize,
}
impl Token {
    fn is(self, kind: Kind) -> bool {
        self.kind == kind
    }
    fn symbol(self, c: u8) -> bool {
        self.is(Kind::Symbol) && self.value == i64::from(c)
    }
    fn fixed(self, n: usize) -> bool {
        self.is(Kind::Number) && self.length == n
    }
    fn sign(self) -> Option<i64> {
        if self.symbol(b'+') {
            Some(1)
        } else if self.symbol(b'-') {
            Some(-1)
        } else {
            None
        }
    }
    fn z(self) -> bool {
        self.is(Kind::Zone) && self.length == 1 && self.value == 0
    }
    fn milliseconds(self) -> i64 {
        match self.length {
            0..=2 => self.value * 10_i64.pow(3 - self.length as u32),
            n => self.value / 10_i64.pow(n.min(9) as u32 - 3),
        }
    }
}

struct Scanner<'a> {
    input: &'a [u16],
    index: usize,
    peek: Token,
}
impl<'a> Scanner<'a> {
    fn new(input: &'a [u16]) -> Self {
        let mut scanner = Self {
            input,
            index: 0,
            peek: Token {
                kind: Kind::End,
                value: 0,
                length: 0,
            },
        };
        scanner.peek = scanner.scan();
        scanner
    }
    fn ch(&self) -> u16 {
        self.input.get(self.index).copied().unwrap_or(0)
    }
    fn next(&mut self) -> Token {
        let token = self.peek;
        self.peek = self.scan();
        token
    }
    fn skip(&mut self, c: u8) -> bool {
        if self.peek.symbol(c) {
            self.next();
            true
        } else {
            false
        }
    }
    fn scan(&mut self) -> Token {
        let start = self.index;
        let ch = self.ch();
        let (kind, value) = match ch {
            0 => (Kind::End, 0),
            48..=57 => {
                while self.ch() == 48 {
                    self.index += 1;
                }
                let mut n = 0;
                let mut digits = 0;
                while (48..=57).contains(&self.ch()) {
                    if digits < 9 {
                        n = n * 10 + i64::from(self.ch() - 48);
                    }
                    digits += 1;
                    self.index += 1;
                }
                (Kind::Number, n)
            }
            58 | 45 | 43 | 46 | 41 => {
                self.index += 1;
                (Kind::Symbol, i64::from(ch))
            }
            c if c >= 65 && !white_space(c) => {
                let mut prefix = [0; 3];
                let mut length = 0;
                while self.ch() >= 65 && !white_space(self.ch()) {
                    if length < 3 {
                        let c = self.ch();
                        prefix[length] = if (65..=90).contains(&c) { c + 32 } else { c };
                    }
                    length += 1;
                    self.index += 1;
                }
                keyword(prefix, length)
            }
            c if crate::string::is_js_whitespace_unit(c) => {
                self.index += 1;
                (Kind::Space, 0)
            }
            40 => {
                let mut balance = 0;
                loop {
                    match self.ch() {
                        40 => balance += 1,
                        41 => balance -= 1,
                        _ => {}
                    }
                    self.index += 1;
                    if balance == 0 || self.ch() == 0 {
                        break;
                    }
                }
                (Kind::Unknown, 0)
            }
            _ => {
                self.index += 1;
                (Kind::Unknown, 0)
            }
        };
        Token {
            kind,
            value,
            length: self.index - start,
        }
    }
}

fn white_space(c: u16) -> bool {
    crate::string::is_js_whitespace_unit(c) && !matches!(c, 10 | 13 | 0x2028 | 0x2029)
}
fn keyword(prefix: [u16; 3], length: usize) -> (Kind, i64) {
    const WORDS: &[(&[u8], Kind, i64)] = &[
        (b"jan", Kind::Month, 1),
        (b"feb", Kind::Month, 2),
        (b"mar", Kind::Month, 3),
        (b"apr", Kind::Month, 4),
        (b"may", Kind::Month, 5),
        (b"jun", Kind::Month, 6),
        (b"jul", Kind::Month, 7),
        (b"aug", Kind::Month, 8),
        (b"sep", Kind::Month, 9),
        (b"oct", Kind::Month, 10),
        (b"nov", Kind::Month, 11),
        (b"dec", Kind::Month, 12),
        (b"am", Kind::AmPm, 0),
        (b"pm", Kind::AmPm, 12),
        (b"ut", Kind::Zone, 0),
        (b"utc", Kind::Zone, 0),
        (b"z", Kind::Zone, 0),
        (b"gmt", Kind::Zone, 0),
        (b"cdt", Kind::Zone, -5),
        (b"cst", Kind::Zone, -6),
        (b"edt", Kind::Zone, -4),
        (b"est", Kind::Zone, -5),
        (b"mdt", Kind::Zone, -6),
        (b"mst", Kind::Zone, -7),
        (b"pdt", Kind::Zone, -7),
        (b"pst", Kind::Zone, -8),
        (b"t", Kind::Separator, 0),
    ];
    for &(word, kind, value) in WORDS {
        let mut padded = [0; 3];
        for (i, &c) in word.iter().enumerate() {
            padded[i] = u16::from(c);
        }
        if prefix == padded && (length <= 3 || kind == Kind::Month) {
            return (kind, value);
        }
    }
    (Kind::Invalid, 0) // Unknown words are keyword tokens, not terminal parse errors.
}

#[derive(Default)]
struct Day {
    values: Vec<i64>,
    month: Option<i64>,
    iso: bool,
}
impl Day {
    fn add(&mut self, n: i64) -> Option<()> {
        if self.values.len() == 3 {
            return None;
        }
        self.values.push(n);
        Some(())
    }
    fn write(mut self) -> Option<(i64, i64, i64)> {
        if self.values.is_empty() {
            return None;
        }
        self.values.resize(3, 1);
        let [a, b, c] = self.values[..] else { unreachable!() };
        // V8 fills missing fields before choosing the ordering (index is now 3).
        let (mut year, month, day) = if let Some(month) = self.month {
            if !(1..=31).contains(&a) {
                (a, month, b)
            } else {
                (b, month, a)
            }
        } else if self.iso || !(1..=31).contains(&a) {
            (a, b, c)
        } else {
            (c, a, b)
        };
        if !self.iso {
            if (0..=49).contains(&year) {
                year += 2000;
            } else if (50..=99).contains(&year) {
                year += 1900;
            }
        }
        if i32::try_from(year).is_err() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        Some((year, month, day))
    }
}
#[derive(Default)]
struct Time {
    values: Vec<i64>,
    hour_offset: Option<i64>,
}
impl Time {
    fn add(&mut self, n: i64) -> Option<()> {
        if self.values.len() == 4 {
            return None;
        }
        self.values.push(n);
        Some(())
    }
    fn final_value(&mut self, n: i64) {
        if self.add(n).is_some() {
            self.values.resize(4, 0);
        }
    }
    fn expecting(&self, n: i64) -> bool {
        match self.values.len() {
            1 | 2 => (0..=59).contains(&n),
            3 => (0..=999).contains(&n),
            _ => false,
        }
    }
    fn write(mut self) -> Option<[i64; 4]> {
        self.values.resize(4, 0);
        let [mut h, m, s, ms] = self.values[..] else {
            unreachable!()
        };
        if let Some(offset) = self.hour_offset {
            if !(0..=12).contains(&h) {
                return None;
            }
            h = h % 12 + offset;
        }
        if !((0..=23).contains(&h) && (0..=59).contains(&m) && (0..=59).contains(&s) && (0..=999).contains(&ms))
            && !(h == 24 && m == 0 && s == 0 && ms == 0)
        {
            return None;
        }
        Some([h, m, s, ms])
    }
}
#[derive(Default)]
struct Zone {
    sign: Option<i64>,
    hour: Option<i64>,
    minute: Option<i64>,
}
impl Zone {
    fn set(&mut self, h: i64) {
        self.sign = Some(if h < 0 { -1 } else { 1 });
        self.hour = Some(h.abs());
        self.minute = Some(0);
    }
    fn utc(&self) -> bool {
        self.hour == Some(0) && self.minute == Some(0)
    }
    fn expecting(&self, n: i64) -> bool {
        self.hour.is_some() && self.minute.is_none() && (0..=59).contains(&n)
    }
    fn write(self) -> Option<Option<i64>> {
        let Some(sign) = self.sign else {
            return Some(None);
        };
        // V8 performs uint32 arithmetic before checking the reference Node build's 32-bit Smi range.
        let seconds = (self.hour.unwrap_or(0) as u32)
            .wrapping_mul(3600)
            .wrapping_add((self.minute.unwrap_or(0) as u32).wrapping_mul(60));
        if seconds > i32::MAX as u32 {
            return None;
        }
        Some(Some(sign * i64::from(seconds)))
    }
}

pub(super) struct Parsed {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub time: [i64; 4],
    pub offset: Option<i64>,
}
pub(super) fn parse(input: &[u16]) -> Option<Parsed> {
    let mut scanner = Scanner::new(input);
    let mut day = Day::default();
    let mut time = Time::default();
    let mut zone = Zone::default();
    let mut token = parse_iso(&mut scanner, &mut day, &mut time, &mut zone)?;
    let mut has_number = !day.values.is_empty();
    while !token.is(Kind::End) {
        if token.is(Kind::Number) {
            has_number = true;
            let n = token.value;
            if scanner.skip(b':') {
                if scanner.skip(b':') {
                    if !time.values.is_empty() {
                        return None;
                    }
                    time.add(n)?;
                    time.add(0)?;
                } else {
                    time.add(n)?;
                    if scanner.peek.symbol(b'.') {
                        scanner.next();
                    }
                }
            } else if scanner.skip(b'.') && time.expecting(n) {
                time.add(n)?;
                if !scanner.peek.is(Kind::Number) {
                    return None;
                }
                time.final_value(scanner.next().milliseconds());
            } else if zone.expecting(n) {
                zone.minute = Some(n);
            } else if time.expecting(n) {
                time.final_value(n);
                let peek = scanner.peek;
                if !peek.is(Kind::End) && !peek.is(Kind::Space) && !peek.z() && peek.sign().is_none() {
                    return None;
                }
            } else {
                day.add(n)?;
                scanner.skip(b'-');
            }
        } else if matches!(
            token.kind,
            Kind::Invalid | Kind::Month | Kind::Zone | Kind::AmPm | Kind::Separator
        ) {
            match token.kind {
                Kind::AmPm if !time.values.is_empty() => time.hour_offset = Some(token.value),
                Kind::Month => {
                    day.month = Some(token.value);
                    scanner.skip(b'-');
                }
                Kind::Zone if has_number => zone.set(token.value),
                _ => {
                    if has_number || scanner.peek.is(Kind::Number) {
                        return None;
                    }
                }
            }
        } else if let Some(sign) = token.sign().filter(|_| zone.utc() || !time.values.is_empty()) {
            zone.sign = Some(sign);
            let next = if scanner.peek.is(Kind::Number) {
                scanner.next()
            } else {
                Token {
                    kind: Kind::Number,
                    value: 0,
                    length: 0,
                }
            };
            has_number = true;
            if scanner.peek.symbol(b':') {
                zone.hour = Some(next.value);
                zone.minute = None;
            } else if matches!(next.length, 1 | 2) {
                zone.hour = Some(next.value);
                zone.minute = Some(0);
            } else if matches!(next.length, 3 | 4) {
                zone.hour = Some(next.value / 100);
                zone.minute = Some(next.value % 100);
            } else {
                return None;
            }
        } else if (token.sign().is_some() || token.symbol(b')')) && has_number {
            return None;
        }
        token = scanner.next();
    }
    let (year, month, day) = day.write()?;
    Some(Parsed {
        year,
        month,
        day,
        time: time.write()?,
        offset: zone.write()?,
    })
}

// A returned token resumes legacy parsing; None is a terminal ISO time error.
fn parse_iso(s: &mut Scanner<'_>, day: &mut Day, time: &mut Time, zone: &mut Zone) -> Option<Token> {
    if let Some(sign) = s.peek.sign() {
        let token = s.next();
        if !s.peek.fixed(6) {
            return Some(token);
        }
        let year = s.next().value;
        if sign < 0 && year == 0 {
            return Some(token);
        }
        day.add(sign * year)?;
    } else if s.peek.fixed(4) {
        day.add(s.next().value)?;
    } else {
        return Some(s.next());
    }
    if s.skip(b'-') {
        if !s.peek.fixed(2) || !(1..=12).contains(&s.peek.value) {
            return Some(s.next());
        }
        day.add(s.next().value)?;
        if s.skip(b'-') {
            if !s.peek.fixed(2) || !(1..=31).contains(&s.peek.value) {
                return Some(s.next());
            }
            day.add(s.next().value)?;
        }
    }
    if !s.peek.is(Kind::Separator) {
        if !s.peek.is(Kind::End) {
            return Some(s.next());
        }
    } else {
        s.next();
        let hour = take_fixed(s, 2, 24)?;
        time.add(hour)?;
        if !s.skip(b':') {
            return None;
        }
        let minute = take_fixed(s, 2, 59)?;
        if hour == 24 && minute != 0 {
            return None;
        }
        time.add(minute)?;
        if s.skip(b':') {
            let second = take_fixed(s, 2, 59)?;
            if hour == 24 && second != 0 {
                return None;
            }
            time.add(second)?;
            if s.skip(b'.') {
                if !s.peek.is(Kind::Number) || (hour == 24 && s.peek.value > 0) {
                    return None;
                }
                time.add(s.next().milliseconds())?;
            }
        }
        if s.peek.z() {
            s.next();
            zone.set(0);
        } else if let Some(sign) = s.peek.sign() {
            s.next();
            zone.sign = Some(sign);
            if s.peek.fixed(4) {
                let n = s.next().value;
                if n / 100 > 23 || n % 100 > 59 {
                    return None;
                }
                zone.hour = Some(n / 100);
                zone.minute = Some(n % 100);
            } else {
                zone.hour = Some(take_fixed(s, 2, 23)?);
                if !s.skip(b':') {
                    return None;
                }
                zone.minute = Some(take_fixed(s, 2, 59)?);
            }
        }
        if !s.peek.is(Kind::End) {
            return None;
        }
    }
    if zone.hour.is_none() && time.values.is_empty() {
        zone.set(0);
    }
    day.iso = true;
    Some(Token {
        kind: Kind::End,
        value: 0,
        length: 0,
    })
}
fn take_fixed(s: &mut Scanner<'_>, length: usize, max: i64) -> Option<i64> {
    if !s.peek.fixed(length) || s.peek.value > max {
        return None;
    }
    Some(s.next().value)
}

// Copyright (c) 2001, 2002, 2003, 2004, 2005, 2006, 2007, 2008, 2009, 2010,
// 2011, 2012, 2013, 2014, 2015, 2016, 2017, 2018 Python Software Foundation;
// All Rights Reserved.
// License: docs/porting/vendor/V8-array-sort-PSF-LICENSE.
// Changes: Rust port of Node v26.11.1's V8 array-sort.tq; dense typed slices
// use an index workarray and a final permutation instead of JS object accessors.

//! Pinned V8 TimSort for comparators that can be inconsistent (Date NaN).
//! Keep ordinary Rust stable sort for consistent comparators.

const MIN_GALLOP_WINS: usize = 7;
const MIN_RUN_THRESHOLD: usize = 64;
const BINARY_INSERTION_THRESHOLD: usize = 8;

/// Sort a dense typed slice with pinned V8 comparison order and result coercion.
/// NaN and signed zero compare equal; other numbers contribute only their sign.
/// Values stay in place while the comparator runs, as V8 sorts a workarray first.
pub fn js_array_sort<T>(values: &mut [T], mut compare: impl FnMut(&T, &T) -> f64) {
    if values.len() < 2 {
        return;
    }
    let mut order = {
        let mut state = SortState {
            work: (0..values.len()).collect(),
            pending: Vec::new(),
            min_gallop: MIN_GALLOP_WINS,
            compare: |left, right| compare(&values[left], &values[right]),
        };
        state.sort();
        state.work
    };
    for start in 0..order.len() {
        let mut current = start;
        loop {
            let next = order[current];
            order[current] = current;
            if next == start {
                break;
            }
            values.swap(current, next);
            current = next;
        }
    }
}

fn compare_sign(compare: &mut impl FnMut(usize, usize) -> f64, left: usize, right: usize) -> i8 {
    let result = compare(left, right);
    if result < 0.0 {
        -1
    } else if result > 0.0 {
        1
    } else {
        0
    }
}

#[derive(Clone, Copy)]
struct Run {
    base: usize,
    length: usize,
}

struct SortState<C> {
    work: Vec<usize>,
    pending: Vec<Run>,
    min_gallop: usize,
    compare: C,
}

impl<C: FnMut(usize, usize) -> f64> SortState<C> {
    fn sort(&mut self) {
        let length = self.work.len();
        if length < BINARY_INSERTION_THRESHOLD {
            self.binary_insertion_sort(0, 0, length);
            return;
        }
        let min_run = compute_min_run(length);
        let mut remaining = length;
        let mut low = 0;
        while remaining != 0 {
            let mut run_length = self.count_and_make_run(low, low + remaining);
            if run_length < min_run {
                let forced = min_run.min(remaining);
                self.binary_insertion_sort(low, low + run_length, low + forced);
                run_length = forced;
            }
            self.pending.push(Run {
                base: low,
                length: run_length,
            });
            self.merge_collapse();
            low += run_length;
            remaining -= run_length;
        }
        self.merge_force_collapse();
    }

    fn binary_insertion_sort(&mut self, low: usize, start: usize, high: usize) {
        let start = if low == start { start + 1 } else { start };
        for start in start..high {
            let pivot = self.work[start];
            let mut left = low;
            let mut right = start;
            while left < right {
                let middle = left + ((right - left) >> 1);
                if compare_sign(&mut self.compare, pivot, self.work[middle]) < 0 {
                    right = middle;
                } else {
                    left = middle + 1;
                }
            }
            self.work.copy_within(left..start, left + 1);
            self.work[left] = pivot;
        }
    }

    fn count_and_make_run(&mut self, low: usize, high: usize) -> usize {
        if low + 1 == high {
            return 1;
        }
        let mut run_length = 2;
        let mut previous = self.work[low + 1];
        let descending = compare_sign(&mut self.compare, previous, self.work[low]) < 0;
        for index in low + 2..high {
            let current = self.work[index];
            let order = compare_sign(&mut self.compare, current, previous);
            if (descending && order >= 0) || (!descending && order < 0) {
                break;
            }
            previous = current;
            run_length += 1;
        }
        if descending {
            self.work[low..low + run_length].reverse();
        }
        run_length
    }

    fn invariant_established(&self, run: usize) -> bool {
        run < 2 || self.pending[run - 2].length > self.pending[run - 1].length + self.pending[run].length
    }

    fn merge_collapse(&mut self) {
        while self.pending.len() > 1 {
            let mut run = self.pending.len() - 2;
            if !self.invariant_established(run + 1) || !self.invariant_established(run) {
                if self.pending[run - 1].length < self.pending[run + 1].length {
                    run -= 1;
                }
                self.merge_at(run);
            } else if self.pending[run].length <= self.pending[run + 1].length {
                self.merge_at(run);
            } else {
                break;
            }
        }
    }

    fn merge_force_collapse(&mut self) {
        while self.pending.len() > 1 {
            let mut run = self.pending.len() - 2;
            if run > 0 && self.pending[run - 1].length < self.pending[run + 1].length {
                run -= 1;
            }
            self.merge_at(run);
        }
    }

    fn merge_at(&mut self, run: usize) {
        let Run {
            base: mut base_a,
            length: mut length_a,
        } = self.pending[run];
        let Run {
            base: base_b,
            length: mut length_b,
        } = self.pending[run + 1];
        self.pending[run].length = length_a + length_b;
        self.pending.remove(run + 1);
        let key_right = self.work[base_b];
        let skipped = gallop_right(&self.work, key_right, base_a, length_a, 0, &mut self.compare);
        base_a += skipped;
        length_a -= skipped;
        if length_a == 0 {
            return;
        }
        let key_left = self.work[base_a + length_a - 1];
        length_b = gallop_left(&self.work, key_left, base_b, length_b, length_b - 1, &mut self.compare);
        if length_b == 0 {
            return;
        }
        if length_a <= length_b {
            self.merge_low(base_a, length_a, base_b, length_b);
        } else {
            self.merge_high(base_a, length_a, base_b, length_b);
        }
    }

    fn merge_low(&mut self, base_a: usize, mut length_a: usize, base_b: usize, mut length_b: usize) {
        let temp = self.work[base_a..base_a + length_a].to_vec();
        let mut destination = base_a;
        let mut cursor_temp = 0;
        let mut cursor_b = base_b;
        self.work[destination] = self.work[cursor_b];
        destination += 1;
        cursor_b += 1;
        length_b -= 1;
        let mut min_gallop = self.min_gallop;
        let copy_b = if length_b == 0 {
            false
        } else if length_a == 1 {
            true
        } else {
            'merge: loop {
                let mut wins_a = 0;
                let mut wins_b = 0;
                loop {
                    let order = compare_sign(&mut self.compare, self.work[cursor_b], temp[cursor_temp]);
                    if order < 0 {
                        self.work[destination] = self.work[cursor_b];
                        destination += 1;
                        cursor_b += 1;
                        wins_b += 1;
                        length_b -= 1;
                        wins_a = 0;
                        if length_b == 0 {
                            break 'merge false;
                        }
                        if wins_b >= min_gallop {
                            break;
                        }
                    } else {
                        self.work[destination] = temp[cursor_temp];
                        destination += 1;
                        cursor_temp += 1;
                        wins_a += 1;
                        length_a -= 1;
                        wins_b = 0;
                        if length_a == 1 {
                            break 'merge true;
                        }
                        if wins_a >= min_gallop {
                            break;
                        }
                    }
                }
                min_gallop += 1;
                let mut first_iteration = true;
                while wins_a >= MIN_GALLOP_WINS || wins_b >= MIN_GALLOP_WINS || first_iteration {
                    first_iteration = false;
                    min_gallop = min_gallop.saturating_sub(1).max(1);
                    self.min_gallop = min_gallop;
                    wins_a = gallop_right(&temp, self.work[cursor_b], cursor_temp, length_a, 0, &mut self.compare);
                    if wins_a > 0 {
                        self.work[destination..destination + wins_a]
                            .copy_from_slice(&temp[cursor_temp..cursor_temp + wins_a]);
                        destination += wins_a;
                        cursor_temp += wins_a;
                        length_a -= wins_a;
                        if length_a == 1 {
                            break 'merge true;
                        }
                        if length_a == 0 {
                            break 'merge false;
                        }
                    }
                    self.work[destination] = self.work[cursor_b];
                    destination += 1;
                    cursor_b += 1;
                    length_b -= 1;
                    if length_b == 0 {
                        break 'merge false;
                    }
                    wins_b = gallop_left(&self.work, temp[cursor_temp], cursor_b, length_b, 0, &mut self.compare);
                    if wins_b > 0 {
                        self.work.copy_within(cursor_b..cursor_b + wins_b, destination);
                        destination += wins_b;
                        cursor_b += wins_b;
                        length_b -= wins_b;
                        if length_b == 0 {
                            break 'merge false;
                        }
                    }
                    self.work[destination] = temp[cursor_temp];
                    destination += 1;
                    cursor_temp += 1;
                    length_a -= 1;
                    if length_a == 1 {
                        break 'merge true;
                    }
                }
                min_gallop += 1;
                self.min_gallop = min_gallop;
            }
        };
        if copy_b {
            self.work.copy_within(cursor_b..cursor_b + length_b, destination);
            self.work[destination + length_b] = temp[cursor_temp];
        } else if length_a > 0 {
            self.work[destination..destination + length_a].copy_from_slice(&temp[cursor_temp..cursor_temp + length_a]);
        }
    }

    fn merge_high(&mut self, base_a: usize, mut length_a: usize, base_b: usize, mut length_b: usize) {
        let temp = self.work[base_b..base_b + length_b].to_vec();
        let mut destination = (base_b + length_b - 1) as isize;
        let mut cursor_temp = (length_b - 1) as isize;
        let mut cursor_a = (base_a + length_a - 1) as isize;
        self.work[destination as usize] = self.work[cursor_a as usize];
        destination -= 1;
        cursor_a -= 1;
        length_a -= 1;
        let mut min_gallop = self.min_gallop;
        let copy_a = if length_a == 0 {
            false
        } else if length_b == 1 {
            true
        } else {
            'merge: loop {
                let mut wins_a = 0;
                let mut wins_b = 0;
                loop {
                    let order = compare_sign(
                        &mut self.compare,
                        temp[cursor_temp as usize],
                        self.work[cursor_a as usize],
                    );
                    if order < 0 {
                        self.work[destination as usize] = self.work[cursor_a as usize];
                        destination -= 1;
                        cursor_a -= 1;
                        wins_a += 1;
                        length_a -= 1;
                        wins_b = 0;
                        if length_a == 0 {
                            break 'merge false;
                        }
                        if wins_a >= min_gallop {
                            break;
                        }
                    } else {
                        self.work[destination as usize] = temp[cursor_temp as usize];
                        destination -= 1;
                        cursor_temp -= 1;
                        wins_b += 1;
                        length_b -= 1;
                        wins_a = 0;
                        if length_b == 1 {
                            break 'merge true;
                        }
                        if wins_b >= min_gallop {
                            break;
                        }
                    }
                }
                min_gallop += 1;
                let mut first_iteration = true;
                while wins_a >= MIN_GALLOP_WINS || wins_b >= MIN_GALLOP_WINS || first_iteration {
                    first_iteration = false;
                    min_gallop = min_gallop.saturating_sub(1).max(1);
                    self.min_gallop = min_gallop;
                    let position = gallop_right(
                        &self.work,
                        temp[cursor_temp as usize],
                        base_a,
                        length_a,
                        length_a - 1,
                        &mut self.compare,
                    );
                    wins_a = length_a - position;
                    if wins_a > 0 {
                        destination -= wins_a as isize;
                        cursor_a -= wins_a as isize;
                        let source = (cursor_a + 1) as usize;
                        self.work
                            .copy_within(source..source + wins_a, (destination + 1) as usize);
                        length_a -= wins_a;
                        if length_a == 0 {
                            break 'merge false;
                        }
                    }
                    self.work[destination as usize] = temp[cursor_temp as usize];
                    destination -= 1;
                    cursor_temp -= 1;
                    length_b -= 1;
                    if length_b == 1 {
                        break 'merge true;
                    }
                    let position = gallop_left(
                        &temp,
                        self.work[cursor_a as usize],
                        0,
                        length_b,
                        length_b - 1,
                        &mut self.compare,
                    );
                    wins_b = length_b - position;
                    if wins_b > 0 {
                        destination -= wins_b as isize;
                        cursor_temp -= wins_b as isize;
                        let source = (cursor_temp + 1) as usize;
                        let target = (destination + 1) as usize;
                        self.work[target..target + wins_b].copy_from_slice(&temp[source..source + wins_b]);
                        length_b -= wins_b;
                        if length_b == 1 {
                            break 'merge true;
                        }
                        if length_b == 0 {
                            break 'merge false;
                        }
                    }
                    self.work[destination as usize] = self.work[cursor_a as usize];
                    destination -= 1;
                    cursor_a -= 1;
                    length_a -= 1;
                    if length_a == 0 {
                        break 'merge false;
                    }
                }
                min_gallop += 1;
                self.min_gallop = min_gallop;
            }
        };
        if copy_a {
            destination -= length_a as isize;
            cursor_a -= length_a as isize;
            let source = (cursor_a + 1) as usize;
            self.work
                .copy_within(source..source + length_a, (destination + 1) as usize);
            self.work[destination as usize] = temp[cursor_temp as usize];
        } else if length_b > 0 {
            let target = (destination - (length_b as isize - 1)) as usize;
            self.work[target..target + length_b].copy_from_slice(&temp[..length_b]);
        }
    }
}

fn compute_min_run(mut length: usize) -> usize {
    let mut shifted_bit = 0;
    while length >= MIN_RUN_THRESHOLD {
        shifted_bit |= length & 1;
        length >>= 1;
    }
    length + shifted_bit
}

fn gallop_left(
    array: &[usize],
    key: usize,
    base: usize,
    length: usize,
    hint: usize,
    compare: &mut impl FnMut(usize, usize) -> f64,
) -> usize {
    let hint = hint as isize;
    let mut last_offset = 0isize;
    let mut offset = 1isize;
    let order = compare_sign(compare, array[base + hint as usize], key);
    if order < 0 {
        let maximum = length as isize - hint;
        while offset < maximum {
            if compare_sign(compare, array[base + (hint + offset) as usize], key) >= 0 {
                break;
            }
            last_offset = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        offset = offset.min(maximum);
        last_offset += hint;
        offset += hint;
    } else {
        let maximum = hint + 1;
        while offset < maximum {
            if compare_sign(compare, array[base + (hint - offset) as usize], key) < 0 {
                break;
            }
            last_offset = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        offset = offset.min(maximum);
        let previous = last_offset;
        last_offset = hint - offset;
        offset = hint - previous;
    }
    last_offset += 1;
    while last_offset < offset {
        let middle = last_offset + ((offset - last_offset) >> 1);
        if compare_sign(compare, array[base + middle as usize], key) < 0 {
            last_offset = middle + 1;
        } else {
            offset = middle;
        }
    }
    offset as usize
}

fn gallop_right(
    array: &[usize],
    key: usize,
    base: usize,
    length: usize,
    hint: usize,
    compare: &mut impl FnMut(usize, usize) -> f64,
) -> usize {
    let hint = hint as isize;
    let mut last_offset = 0isize;
    let mut offset = 1isize;
    let order = compare_sign(compare, key, array[base + hint as usize]);
    if order < 0 {
        let maximum = hint + 1;
        while offset < maximum {
            if compare_sign(compare, key, array[base + (hint - offset) as usize]) >= 0 {
                break;
            }
            last_offset = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        offset = offset.min(maximum);
        let previous = last_offset;
        last_offset = hint - offset;
        offset = hint - previous;
    } else {
        let maximum = length as isize - hint;
        while offset < maximum {
            if compare_sign(compare, key, array[base + (hint + offset) as usize]) < 0 {
                break;
            }
            last_offset = offset;
            offset = offset.saturating_mul(2).saturating_add(1);
        }
        offset = offset.min(maximum);
        last_offset += hint;
        offset += hint;
    }
    last_offset += 1;
    while last_offset < offset {
        let middle = last_offset + ((offset - last_offset) >> 1);
        if compare_sign(compare, key, array[base + middle as usize]) < 0 {
            offset = middle;
        } else {
            last_offset = middle + 1;
        }
    }
    offset as usize
}

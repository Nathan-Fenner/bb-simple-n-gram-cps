use std::collections::{BTreeMap, BTreeSet};

use crate::program::{Bit, Dir, LoopsForever, MayHalt, Program, State};

/**
* n-grams may go up to 15 bits.
* (32 bits is not allowed because of the context size)
*/
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, PartialOrd, Ord)]
struct NGram(NGramBits);
type NGramBits = u32;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, PartialOrd, Ord)]
struct LocalContext {
    state: State,
    nearby_bits: u64,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct Radius(u8);

impl LocalContext {
    fn push_left(self, bit: Bit, radius: Radius) -> Self {
        LocalContext {
            state: self.state,
            nearby_bits: (self.nearby_bits << 1 | u64::from(bit.0)) & !(1 << (radius.0 * 2 + 1)),
        }
    }
    fn push_right(self, bit: Bit, radius: Radius) -> Self {
        LocalContext {
            state: self.state,
            nearby_bits: self.nearby_bits >> 1 | (if bit.0 { 1 << (2 * radius.0) } else { 0 }),
        }
    }
    fn push(self, dir: Dir, bit: Bit, radius: Radius) -> Self {
        match dir {
            Dir::Left => self.push_left(bit, radius),
            Dir::Right => self.push_right(bit, radius),
        }
    }
    fn write_center(self, bit: Bit, state: State, radius: Radius) -> Self {
        LocalContext {
            state,
            nearby_bits: (self.nearby_bits & !(1 << radius.0))
                | (if bit.0 { 1 << radius.0 } else { 0 }),
        }
    }
    fn get_center(self, radius: Radius) -> Bit {
        Bit((self.nearby_bits & (1 << radius.0)) != 0)
    }
    fn get_left(self, radius: Radius) -> NGram {
        NGram((self.nearby_bits & ((1 << radius.0) - 1)) as NGramBits)
    }
    fn get_right(self, radius: Radius) -> NGram {
        NGram(((self.nearby_bits >> (radius.0 + 1)) & ((1 << radius.0) - 1)) as NGramBits)
    }
    fn get(self, dir: Dir, radius: Radius) -> NGram {
        match dir {
            Dir::Left => self.get_left(radius),
            Dir::Right => self.get_right(radius),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, PartialOrd, Ord)]
struct DirMap<T> {
    left: T,
    right: T,
}

impl<T> DirMap<T> {
    fn new(item: T) -> Self
    where
        T: Clone,
    {
        DirMap {
            left: item.clone(),
            right: item,
        }
    }
}

impl<T> std::ops::Index<Dir> for DirMap<T> {
    type Output = T;
    fn index(&self, index: Dir) -> &Self::Output {
        match index {
            Dir::Left => &self.left,
            Dir::Right => &self.right,
        }
    }
}
impl<T> std::ops::IndexMut<Dir> for DirMap<T> {
    fn index_mut(&mut self, index: Dir) -> &mut Self::Output {
        match index {
            Dir::Left => &mut self.left,
            Dir::Right => &mut self.right,
        }
    }
}

struct PartialReachable {
    radius: Radius, // must lie in [1, 31]
    reachable_local_contexts: BTreeSet<LocalContext>,
    reachable_ngrams: DirMap<BTreeSet<NGram>>,
}

impl PartialReachable {
    fn new(radius: u8) -> Self {
        if !(1..=31).contains(&radius) {
            panic!("PartialReachable radius must lie in [1, 31]");
        }
        PartialReachable {
            radius: Radius(radius),
            reachable_local_contexts: [LocalContext {
                state: State(1),
                nearby_bits: 0,
            }]
            .into_iter()
            .collect(),
            reachable_ngrams: DirMap::new({
                let mut res = BTreeSet::new();
                res.insert(NGram(0));
                res
            }),
        }
    }

    /**
     * Checks to see if an extension is needed to capture all reachable states.
     * If so, returns true and adds some of them.
     * Call this method repeatedly until false to ensure that we capture all of them.
     */
    fn check_if_closed_under_program_step(&self, program: &Program) -> bool {
        for local_context in self.reachable_local_contexts.iter() {
            // For this local context, see what the program says to do.
            let action =
                match program.action(local_context.get_center(self.radius), local_context.state) {
                    Ok(action) => action,
                    _ => return false,
                };

            // Suppose the action says to move left. This is the naming convention we use:
            let dir = action.2;

            // Since we are moving "left", the opposite side (right) must have an ngram "fall off" of the local context.
            let ngram_falling_off_right = local_context.get(dir.opposite(), self.radius);
            if !self.reachable_ngrams[dir.opposite()].contains(&ngram_falling_off_right) {
                // If we don't already have `ngram_falling_off_right` marked as reachable, fix that by marking it reachable.
                // Since we extended the set of reachable things, we also have to start over and check them all again.
                return false;
            }

            // A single step causes us to write the center bit, and then push a new bit onto the left.
            // We don't know what that bit is, just that it's either 0 or 1. Therefore, we separately
            // check both cases.

            for discovered_bit in [Bit(false), Bit(true)] {
                // If the pushed bit is 0, then we check whether the new left-half of the context is known.
                // If it is not known, then we cannot reach this context, so we can skip it.
                // But if the left half is known, then this new context can be reached in a single step.
                let discovered_context = local_context
                    .write_center(action.1, action.0, self.radius)
                    .push(dir, discovered_bit, self.radius);
                if self.reachable_ngrams[dir].contains(&discovered_context.get(dir, self.radius))
                    && !self.reachable_local_contexts.contains(&discovered_context)
                {
                    // When the left half is known but the context as a whole is not, mark it as known
                    // and start over.
                    return false;
                }
            }
        }

        true
    }

    /**
     * Adds more, to quickly saturate, does not check for saturation.
     */
    fn add_to_saturate_quick(&mut self, program: &Program, max_context_count: usize) {
        let mut work_queue_local: Vec<LocalContext> =
            self.reachable_local_contexts.iter().cloned().collect();

        let mut work_queue_grams: DirMap<BTreeMap<NGram, Vec<LocalContext>>> =
            DirMap::new(BTreeMap::new());

        while let Some(local_context) = work_queue_local.pop() {
            if self.reachable_local_contexts.len() > max_context_count {
                // Give up, it has taken too long.
                return;
            }

            let action =
                match program.action(local_context.get_center(self.radius), local_context.state) {
                    Ok(action) => action,
                    _ => {
                        // Stop, since we hit a halting state.
                        return;
                    }
                };

            // Suppose the action says to move left. This is the naming convention we use:
            let dir = action.2;

            // Since we are moving "left", the opposite side (right) must have an ngram "fall off" of the local context.
            let ngram_falling_off_right = local_context.get(dir.opposite(), self.radius);
            if !self.reachable_ngrams[dir.opposite()].contains(&ngram_falling_off_right) {
                // If we don't already have `ngram_falling_off_right` marked as reachable, fix that by marking it reachable.
                // Since we extended the set of reachable things, we also have to start over and check them all again.
                self.reachable_ngrams[dir.opposite()].insert(ngram_falling_off_right);

                if work_queue_grams[dir.opposite()].contains_key(&ngram_falling_off_right) {
                    // Move all of these items into the main queue.
                    for revisit_local in work_queue_grams[dir.opposite()]
                        .remove(&ngram_falling_off_right)
                        .unwrap()
                    {
                        // Revisit this one, since it was waiting on this ngram being available.
                        work_queue_local.push(revisit_local);
                    }
                }
            }

            // A single step causes us to write the center bit, and then push a new bit onto the left.
            // We don't know what that bit is, just that it's either 0 or 1. Therefore, we separately
            // check both cases.

            for discovered_bit in [Bit(false), Bit(true)] {
                // If the pushed bit is 0, then we check whether the new left-half of the context is known.
                // If it is not known, then we cannot reach this context, so we can skip it.
                // But if the left half is known, then this new context can be reached in a single step.
                let discovered_context = local_context
                    .write_center(action.1, action.0, self.radius)
                    .push(dir, discovered_bit, self.radius);

                let discovered_ngram = discovered_context.get(dir, self.radius);

                if self.reachable_ngrams[dir].contains(&discovered_ngram) {
                    // When the left half is known but the context as a whole is not, mark it as known
                    // and start over.
                    if !self.reachable_local_contexts.contains(&discovered_context) {
                        self.reachable_local_contexts.insert(discovered_context);
                        work_queue_local.push(discovered_context);
                    }
                } else {
                    // Otherwise, remember that we are waiting on this gram, so that if it appears,
                    // we can revisit things.
                    work_queue_grams[dir]
                        .entry(discovered_ngram)
                        .or_default()
                        .push(local_context);
                }
            }
        }
    }

    fn confirm_closed_under_program(
        &mut self,
        program: &Program,
        max_context_count: usize,
    ) -> Result<LoopsForever, MayHalt> {
        self.add_to_saturate_quick(program, max_context_count);

        if self.check_if_closed_under_program_step(program) {
            Ok(LoopsForever)
        } else {
            Err(MayHalt)
        }
    }
}

impl NGram {
    pub fn print(self, r: Radius) {
        for i in 0..r.0 {
            if (self.0 & (1 << i)) != 0 {
                print!("1");
            } else {
                print!("0");
            }
        }
    }
}
impl LocalContext {
    pub fn print(self, r: Radius) {
        for i in 0..2 * r.0 + 1 {
            if i == r.0 {
                print!("[");
                print!("{}", (b'A' - 1 + self.state.0) as char);
            }
            if (self.nearby_bits & (1 << i)) != 0 {
                print!("1");
            } else {
                print!("0");
            }
            if i == r.0 {
                print!("]");
            }
        }
    }
}

pub fn classify(
    program: &Program,
    radius: u8,
    max_context_count: usize,
) -> Result<LoopsForever, MayHalt> {
    let mut reachable = PartialReachable::new(radius);
    assert!(radius >= 1);
    assert!(radius <= 31);
    reachable.confirm_closed_under_program(program, max_context_count)
}

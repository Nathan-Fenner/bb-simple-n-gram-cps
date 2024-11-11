# n-GRAM CPS Decider

This program evaluates 5-state Turing Machines in the [bbchallenge format](https://bbchallenge.org/), identifying some of them as looping forever.

## Running the program

```
cargo run --release -- --seed-database ../seed-database --undecided-index ../index-file --radius 5
# or
cargo run --release -- --machine 1RB0RD_1LC1LB_1RA0LB_0RE1RD_---1RA --radius 6
```

## Method Overview

This decider evaluates programs using a fixed `n` radius. In these examples, `n=3` is used.

"Reachable" configurations are abstractly described by `n` bits on either side of the machine head.
In the code, this is called a `LocalContext`. For example, the initial local context is

```
       A
....0000000....
```

A more generic example would be

```
       C
....1101011....
```

In the example program [1RB0RD*1LC1LB_1RA0LB_0RE1RD*---1RA](https://bbchallenge.org/1RB0RD_1LC1LB_1RA0LB_0RE1RD_---1RA), the corresponding action is `0LB`.

So we will write a `0`:

```
       C
....1100011....
```

shift the head left:

```
      C
....1100011....
```

and change the state to `B`

```
      B
....1100011....
```

However, we no longer have a local context.
The known bits are too short on the left and too long on the right.

First, we "drop" the extra bit on the right. In doing so, we record that it belong to the n-gram `011`.
Therefore, the n-gram `011` may be encountered in the future on the right side of the head.

Meanwhile, we add a mystery bit to the left side:

```
       B
....?110101....
```

It could either be `0` or `1`.

- If it's `0`, then we must have written `011` onto the left half of the tape at some point in the past.
- If it's `1`, then we must have written `111` ont the left half of the tape at some point in the past.

So we check whether either or both of these are true (at least one of them has to be) and add the corresponding local context(s) to our search queue.

If we run out of n-grams to add and our search queue is complete, then that means we've found a closed set describing all reachable configurations.
In that case, the machine loops forever.

Note when we encounter a local context "later", we may add an n-gram that an "earlier" local context looked for but did not see; therefore, the actual
decider merely verifies the closure property that all of the expected contexts and n-grams exist, allowing a separate, more-complex algorithm to keep
track of which ones have already been handled and which need to be updated.
This means that the built-in verifier is just checking a closure/fixed-point property.

## Relation to other forms of "CPS"

This decider is similar, but not exactly the same, as other deciders which use CPS.

Since `n`-grams almost entirely overlap each other, there is no need (or benefit) to tracking which `n`-grams are allowed to occur before/after each other;
this is determined exactly by which bit(s) is allowed to precede/follow them, which is exactly what's tracked by `n+1`-grams.

This simplifies the verification logic, since we only need to know the set of `n`-grams on the left and a separate set of `n`-grams on the right, with no need to track relationships among those elements. In addition, the set of reachable local contexts acts as the "glue" connecting these.

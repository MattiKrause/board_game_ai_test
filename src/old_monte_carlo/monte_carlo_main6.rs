use std::marker::PhantomData;
use std::mem::size_of;
use std::time::{Duration, Instant};
use bumpalo::Bump;
use rand::{Rng, RngCore, SeedableRng, thread_rng};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use crate::monte_carlo_game::{MonteCarloGame, Winner};
use crate::{MonteLimit, WinReward};
use crate::ai_infra::GameStrategy;
use crate::monte_carlo_win_reducer::{WinReducer, WinReducerFactory};
use crate::multi_score_reducer::{MultiScoreReducerFactory, ScoreReducer, TwoScoreReducer};

#[allow(dead_code)]
pub struct MonteCarloStrategyV6<G: MonteCarloGame, WRF: MultiScoreReducerFactory<G>> {
    limit: MonteLimit, c: f64, wrf: WRF, seed: Option<[u8; 32]>, game: PhantomData<G>
}

pub struct MonteCarloCarry {
    allocator: Bump,
    playoff_buf: Bump,
    rng: rand::rngs::SmallRng
}


#[derive(Debug)]
struct MonteCarloChild<'b, G: MonteCarloGame>(G::MOVE, Option<MonteCarloState<'b, G>>);

#[derive(Debug)]
struct MonteCarloState<'b, G: MonteCarloGame> {
    children: &'b mut [MonteCarloChild<'b, G>],
    visited: f64,
    wins: f64,
    leaf_count: u16,
    game: G,
    winner: Option<Winner>,
}


impl<'b, G: MonteCarloGame> MonteCarloState<'b, G> {
    fn new(g: G, winner: Option<Winner>, bump: &'b Bump) -> Self {
        let children = if winner.is_none() {
            let moves = g.moves().into_iter();
            let mut children = bumpalo::collections::Vec::with_capacity_in(moves.size_hint().0, bump);
            for m in moves {
                children.push(MonteCarloChild(m, None))
            }
            children
        } else {
            bumpalo::collections::Vec::new_in(bump)
        };
        let children = children.into_bump_slice_mut();
        Self {
            children,
            visited: 0.0,
            wins: 0.0,
            leaf_count: 0,
            game: g,
            winner,
        }
    }
}

macro_rules! monte_carlo_loop {
    ($limit: expr, $operations: ident, $action: block) => {
        let mut $operations = 0.0f64;
        match $limit {
            MonteLimit::Duration { millis } => {
                let start = Instant::now();
                let millis = Duration::from_millis(millis.get());
                while start.elapsed() < millis {
                    $operations += 1.0;
                    $action
                }
            }
            MonteLimit::Times { times } => {
                let times = f64::from(times);
                while $operations < times {
                    $operations += 1.0;
                    $action
                }
            }
        }
        println!("operations: {}", $operations);
    };
}

impl<G: MonteCarloGame + 'static, W: MultiScoreReducerFactory<G>> GameStrategy<G> for MonteCarloStrategyV6<G, W> {
    type Carry = MonteCarloCarry;
    type Config = (MonteLimit, f64, W, Option<[u8; 32]>);

    fn new((limit, c, wrf, seed): (MonteLimit, f64, W, Option<[u8; 32]>)) -> Self {
        Self {
            limit,
            c,
            wrf,
            seed,
            game: PhantomData::default(),
        }
    }

    fn make_move(&self, game: &G, carry: Option<(G::MOVE, Self::Carry)>) -> (G::MOVE, Self::Carry) {
        let rng = self.seed.map(|seed| rand::SeedableRng::from_seed(seed)).unwrap_or_else(|| {
            let mut seed = [0; 32];
            thread_rng().fill_bytes(&mut seed);
            SeedableRng::from_seed(seed)
        });
        let mut carry = carry.map(|(_, c)| c).unwrap_or_else(|| MonteCarloCarry {
            allocator: Bump::with_capacity(size_of::<G>() * 50_000),
            playoff_buf: Bump::new(),
            rng,
        });
        let m = make_monte_carlo_move(game, &carry.allocator, &mut carry.playoff_buf, &mut carry.rng, self.limit, self.c, &self.wrf);
        carry.allocator.reset();
        (m, carry)
    }
}

fn make_monte_carlo_move<G: MonteCarloGame + 'static, W: MultiScoreReducerFactory<G>>(g: &G, bump: &Bump, tmp_buf: &mut Bump, rng: &mut impl Rng, limit: MonteLimit, c: f64, wr_factory: &W) -> G::MOVE {
    let mut children = {
        let moves = g.moves().into_iter();
        let mut children = Vec::with_capacity(moves.size_hint().0);
        for m in moves.into_iter() {
            let (s, w) = g.make_move(&m).unwrap();
            if let Some(Winner::WIN) = w {
                return m;
            }
            let new_state = MonteCarloState::new(s, w, &bump);
            children.push(MonteCarloChild(m, Some(new_state)))
        }
        children
    };
    monte_carlo_loop!(limit, operations, {
        let next = select_next(rng, tmp_buf, children.iter_mut(), operations, c);
        let next = if let Some(next) = next {
            next
        } else {
            break;
        };
        playoff(g, next, wr_factory, bump, tmp_buf, rng, c);
    });

    return children
        .into_iter()
        .filter_map(|c| (c.1.map(|s| (c.0, s))))
        .map(|(m, s)| {
            (m, s.wins / s.visited)
        })
        .inspect(|(m, wr)| println!("{m:?}: {wr}"))
        .max_by(|m1, m2| m1.1.total_cmp(&m2.1))
        .unwrap()
        .0;
}

fn playoff<'a, 'b, G: MonteCarloGame + 'static, W: MultiScoreReducerFactory<G>>(
    mut g: &'a G,
    next: &'a mut MonteCarloChild<'b, G>,
    wr_config: &W,
    bump: &'b Bump,
    tmp_buf: &mut Bump,
    rng: &mut impl Rng,
    c: f64,
) {
    let mut path = Vec::with_capacity(30);
    let mut next = next;
    let winner;
    loop {
        let current = match next.1 {
            Some(ref mut child) => child,
            None => {
                let child = g.make_move(&next.0);
                let child = match child {
                    Ok(c) => c,
                    Err(_) => {
                        println!("move: {:?}", next.0);
                        println!("field:\n{g:?}");
                        panic!("invalid move");
                    }
                };
                let next_state = MonteCarloState::new(child.0, child.1, bump);
                next.1 = Some(next_state);
                next.1.as_mut().unwrap()
            }
        };
        current.visited += 1.0;
        g = &current.game;
        if let Some(w) = current.winner {
            winner = w;
            current.leaf_count += 1;
            break;
        }

        let child_count = current.children.len();

        tmp_buf.reset();
        let new = select_next(
            rng,
            tmp_buf,
            current.children.iter_mut(),
            current.visited,
            c,
        ).unwrap();

        path.push((&mut current.wins, &mut current.leaf_count, child_count));
        next = new;
    }

    let mut score_reducer = wr_config.create(g);
    let mut is_leaf = true;
    for (wins, leaf_count, child_count) in path.into_iter().rev() {
        *wins += score_reducer.next_score(child_count);
        *leaf_count += is_leaf as u8 as u16;
        is_leaf = *leaf_count as usize >= child_count;
    }
}

fn select_next<'c: 'd, 'd, 'b: 'c, G: MonteCarloGame + 'static>(
    rng: &mut impl Rng,
    tmp_buf: &Bump,
    mut children: impl Iterator<Item=&'c mut MonteCarloChild<'b, G>>,
    parent_visited: f64, c: f64
) -> Option<&'d mut MonteCarloChild<'b, G>> {
    let children_assume_size = {
        let (lower, upper) = children.size_hint();
        upper.unwrap_or(lower)
    };
    let mut existing = bumpalo::collections::Vec::with_capacity_in(children_assume_size, tmp_buf);
    let mut not_existing = bumpalo::collections::Vec::with_capacity_in(children_assume_size, tmp_buf);

    for child in children {
        match child.1 {
            None => not_existing.push(child),
            Some(ref c) if c.visited == 0.0 => not_existing.push(child),
            Some(ref c) if usize::from(c.leaf_count) < c.children.len() => existing.push(child),
            _ => {}
        }
    }
    if not_existing.len() > 0 {
        let idx = rng.gen_range(0..not_existing.len());
        return Some(not_existing.into_iter().skip(idx).next().expect("sould have result"))
    }

    let parent_factor = parent_visited.ln();

    let mut scores = tmp_buf.alloc_slice_fill_iter(existing.iter().map(|child| {
        let child = child.1.as_ref().unwrap();
        let mut score = (child.wins / child.visited) + c * (parent_factor / child.visited).sqrt();
        debug_assert!(!score.is_nan());
        score
    }));
    let min_values = scores.iter().copied().fold(0.0, f64::min);

    let mut highest_score = 0.0;
    for score in scores.iter_mut() {
        highest_score += *score - min_values + f64::EPSILON;
        *score = highest_score;
    }

    debug_assert!(existing.len() == scores.len());

    if existing.len() > 0 && scores.len() > 0 {
        let _existing = existing.as_slice();
        if highest_score <= 0.0 {
            println!("min_val: {min_values}: {scores:?}, {highest_score}");
        };
        let rng = rng.gen_range(0.0..highest_score);
        let idx = scores.iter().enumerate()
            .find_map(|(i, s)| (rng <= *s).then_some(i));
        let idx = if let Some(idx) = idx {
            idx
        } else {
            let _ = 2 + 2;
            panic!("invalid value");
        };

        Some(existing.into_iter().skip(idx).next().expect("should have result"))
    } else {
        None
    }
}
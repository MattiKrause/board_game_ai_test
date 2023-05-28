use std::marker::PhantomData;
use std::mem::size_of;

use std::time::{Duration, Instant};

use bumpalo::Bump;
use rand::{Rng, RngCore, SeedableRng, thread_rng};



use crate::{MonteLimit};
use crate::ai_infra::GameStrategy;
use crate::monte_carlo_game::{MonteCarloGame, Winner};
use crate::multi_score_reducer::{MultiScoreReducerFactory, ScoreReducer};

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
enum MonteState<'b, G: MonteCarloGame> {
    Computed(MonteCarloState<'b, G>),
    Uncomputed(G::MOVE, &'b G),
}
#[derive(Debug)]
struct MonteCarloChild<'b, G: MonteCarloGame>(MonteState<'b, G>);

#[derive(Debug)]
struct MonteCarloState<'b, G: MonteCarloGame> {
    children: &'b mut [MonteCarloChild<'b, G>],
    visited: f64,
    wins: f64,
    leaf_count: u16,
}


impl<'b, G: MonteCarloGame> MonteCarloState<'b, G> {
    fn new(g: &'b G, winner: Option<Winner>, bump: &'b Bump) -> Self {
        let children = if winner.is_none() {
            let moves = g.moves().into_iter();
            let mut children = bumpalo::collections::Vec::with_capacity_in(moves.size_hint().0, bump);
            children.extend(moves.map(|m| MonteCarloChild(MonteState::Uncomputed(m, g))));
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

fn make_monte_carlo_move<G: MonteCarloGame + 'static, W: MultiScoreReducerFactory<G>>(g: &G, bump: &Bump, tmp_buf: &mut Bump, rng: &mut impl Rng, limit: MonteLimit, c: f64, wr_factory: &W) -> G::MOVE where G::MOVE: Clone{
    let mut children = {
        let moves = g.moves().into_iter();
        let mut children = Vec::with_capacity(moves.size_hint().0);
        for m in moves.into_iter() {
            let (s, w) = g.make_move(&m).unwrap();
            if let Some(Winner::WIN | Winner::TIE) = w {
                return m;
            }
            let s = bump.alloc(s);
            let new_state = MonteCarloState::new(s, w, &bump);
            children.push((m.clone(), MonteCarloChild(MonteState::Computed(new_state))))
        }
        children
    };
    monte_carlo_loop!(limit, operations, {
        let next = select_next(rng, children.iter().map(|(_, s)| s), operations, c);
        let next = if let Some(next) = next {
            next
        } else {
            break;
        };
        let next = &mut children[next].1;
        playoff(next, wr_factory, bump, tmp_buf, rng, c);
    });

    return children
        .into_iter()
        .filter_map(|(m, c)| if let MonteState::Computed(s) = c.0 {
            Some((m, s))
        } else {
            None
        })
        .map(|(m, s)| {
            (m, s.wins / s.visited)
        })
        .inspect(|(m, wr)| println!("{m:?}: {wr}"))
        .max_by(|m1, m2| m1.1.total_cmp(&m2.1))
        .unwrap()
        .0;
}

fn playoff<'a, 'b, G: MonteCarloGame + 'static, W: MultiScoreReducerFactory<G>>(
    next: &'a mut MonteCarloChild<'b, G>,
    wr_config: &W,
    bump: &'b Bump,
    tmp_buf: &mut Bump,
    rng: &mut impl Rng,
    c: f64,
) {
    let mut path = Vec::with_capacity(30);
    let mut next = next;
    let final_game_state;
    loop {
        let mut win_state = None;
        let current = match next.0 {
            MonteState::Computed(ref mut child) => child,
            MonteState::Uncomputed(m, g) => {
                let child = g.make_move(&m);
                let child = match child {
                    Ok(c) => c,
                    Err(_) => {
                        println!("move: {:?}", m);
                        println!("field:\n{g:?}");
                        panic!("invalid move");
                    }
                };

                let g = &*bump.alloc(child.0);
                win_state = child.1.map(|w| (g, w));
                let next_state = MonteCarloState::new(g, child.1, bump);
                next.0 = MonteState::Computed(next_state);
                let MonteState::Computed(ref mut n) = next.0 else { unreachable!() };
                n
            }
        };
        current.visited += 1.0;
        if let Some((g, w)) = win_state {
            final_game_state = g;
            current.leaf_count += 1;
            break;
        }

        let child_count = current.children.len();

        tmp_buf.reset();
        let new = select_next(
            rng,
            current.children.iter(),
            current.visited,
            c,
        );
        let new = if let Some(new) = new {
            new
        } else {
            if path.len() == 0 && current.children.len() == 0 {
                //weird special case when there is only one playable move
                return;
            }
            panic!("alarm: path_len {}, current_leaf_c: {}, current_c_c: {}", path.len(), current.leaf_count, current.children.len());
        };
        let new = &mut current.children[new];

        path.push((&mut current.wins, &mut current.leaf_count, child_count));
        next = new;
    }

    let mut score_reducer = wr_config.create(final_game_state);
    let mut is_leaf = true;
    for (wins, leaf_count, child_count) in path.into_iter().rev() {
        *wins += score_reducer.next_score(child_count);
        *leaf_count += is_leaf as u8 as u16;
        is_leaf = *leaf_count as usize >= child_count;
    }
}

fn select_next<'c: 'd, 'd, 'b: 'c, G: MonteCarloGame + 'static>(
    rng: &mut impl Rng,
    children: impl Iterator<Item=&'c MonteCarloChild<'b, G>>,
    parent_visited: f64, c: f64
) -> Option<usize> {
    let mut max_i = usize::MAX;
    let mut any_uncomputed = false;
    let mut max_score = f64::NEG_INFINITY;
    for (i, child) in children.enumerate() {
        match child.0 {
            MonteState::Computed(MonteCarloState {visited: 0.0, ..}) | MonteState::Uncomputed(_, _) => {
                let rng_score = rng.gen::<f64>();
                if !any_uncomputed || rng_score > max_score {
                    max_score = rng_score;
                    max_i = i;
                }
                any_uncomputed = true;
            }
            MonteState::Computed(ref child) => {
                let score = (child.wins / child.visited) + c * (parent_visited.ln() / child.visited).sqrt();
                if !any_uncomputed && score > max_score && (child.leaf_count as usize) < child.children.len() {
                    max_i = i;
                    max_score = score;
                }
            }
        }
    }

    if max_i < usize::MAX {
        Some(max_i)
    } else {
        None
    }
}
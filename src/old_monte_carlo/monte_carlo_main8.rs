use std::marker::PhantomData;
use std::mem::size_of;

use std::time::{Duration, Instant};

use bumpalo::Bump;
use rand::{Rng, RngCore, SeedableRng, thread_rng};

use rand::seq::SliceRandom;

use crate::{MonteLimit};
use crate::ai_infra::GameStrategy;
use crate::monte_carlo_game_v2::{GameState, MonteCarloGameND};

use crate::multi_score_reducer::{ExecutionLimiter, ExecutionLimiterFactory, MultiScoreReducerFactory, ScoreReducer};

#[allow(dead_code)]
pub struct MonteCarloStrategyV8<G, WRF> {
    limit: MonteLimit,
    c: f64,
    wrf: WRF,
    seed: Option<[u8; 32]>,
    game: PhantomData<G>,
}

pub struct MonteCarloCarry {
    allocator: Bump,
    playoff_buf: Bump,
    rng: rand::rngs::SmallRng,
}


#[derive(Debug)]
enum MonteCarloChild<'b, G: MonteCarloGameND> {
    Computed(MonteCarloMove<'b, G>),
    Uncomputed(G::MOVE),
}

#[derive(Debug)]
struct MonteCarloMove<'b, G: MonteCarloGameND> {
    outcomes: &'b mut [(f64, MonteCarloOutcome<'b, G>)],
    visits: u32,
    non_leaf_count: u16,
    score: f64,
}

#[derive(Debug)]
enum MonteCarloOutcome<'b, G: MonteCarloGameND> {
    Computed(MonteCarloState<'b, G>),
    Uncomputed(G::MOVE, G::Outcome),
}

#[derive(Debug)]
struct MonteCarloState<'b, G: MonteCarloGameND> {
    children: &'b mut [MonteCarloChild<'b, G>],
    non_leaf_count: u16,
    game: &'b G,
}


impl<'b, G: MonteCarloGameND> MonteCarloState<'b, G> {
    fn new(rng: &mut impl Rng, g: &'b G, ended: bool, bump: &'b Bump) -> Self {
        let children = if !ended {
            let moves = g.moves().into_iter();
            let mut children = bumpalo::collections::Vec::with_capacity_in(moves.size_hint().0, bump);
            children.extend(moves.map(|m| MonteCarloChild::Uncomputed(m)));
            children.shuffle(rng);
            children
        } else {
            bumpalo::collections::Vec::new_in(bump)
        };
        let children = children.into_bump_slice_mut();
        let children_len = children.len() as u16;
        Self {
            children,
            non_leaf_count: children_len,
            game: g,
        }
    }
}

macro_rules! monte_carlo_loop {
    ($limit: expr, $operations: ident, $action: block) => {
        let mut $operations = 0u32;
        match $limit {
            MonteLimit::Duration { millis } => {
                let start = Instant::now();
                let millis = Duration::from_millis(millis.get());
                while start.elapsed() < millis {
                    $operations += 1;
                    $action
                }
            }
            MonteLimit::Times { times } => {
                while $operations < times {
                    $operations += 1;
                    $action
                }
            }
        }
        log::debug!("operations: {}", $operations);
    };
}

impl<G: MonteCarloGameND + 'static, W: MultiScoreReducerFactory<G> + ExecutionLimiterFactory<G>> GameStrategy<G> for MonteCarloStrategyV8<G, W> {
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

fn make_monte_carlo_move<G: MonteCarloGameND + 'static, W: MultiScoreReducerFactory<G> + ExecutionLimiterFactory<G>>(g: &G, bump: &Bump, tmp_buf: &mut Bump, rng: &mut impl Rng, limit: MonteLimit, c: f64, wr_factory: &W) -> G::MOVE where G::MOVE: Clone {
    let mut children = {
        let moves = g.moves().into_iter();
        let mut children = Vec::with_capacity(moves.size_hint().0);
        for m in moves.into_iter() {
            children.push((m.clone(), MonteCarloChild::Uncomputed(m)))
        }
        children
    };
    let children_len = children.len();
    let mut non_leaf_count = children.len() as u16;
    monte_carlo_loop!(limit, operations, {
        let next = select_next_move(children.iter().map(|(_, s)| s), operations, c);
        let next = if let Some(next) = next {
            next
        } else {
            break;
        };
        let next = &mut children[next].1;
        playoff(next, g, &mut non_leaf_count, children_len, wr_factory, bump, tmp_buf, rng, c);
    });

    let mut children = children
        .into_iter()
        .filter_map(|(m, c)| if let MonteCarloChild::Computed(s) = c {
            Some((m, s))
        } else {
            None
        })
        .collect::<Vec<_>>();
    let correct_by = (-1.0) * children.iter().map(|(_m, node)| node.score).reduce(f64::min).unwrap_or(0.0);
    children.iter_mut().for_each(|(_, s)| s.score += correct_by);

    let m = children.into_iter()
        .map(|(m, s)| {
            (m, s.visits, s.score / s.visits as f64)
        })
        .inspect(|(m, v, wr)| log::debug!("{m:?}({v}): {wr}"))
        .max_by(|(_, _, wr1), (_, _, wr2)| wr1.total_cmp(&wr2))
        .unwrap()
        .0;
    log::debug!("selected: {m:?}");
    m
}

fn playoff<'a, 'b, G: MonteCarloGameND + 'static, W: MultiScoreReducerFactory<G> + ExecutionLimiterFactory<G>>(
    mut next: &'a mut MonteCarloChild<'b, G>,
    mut game: &'b G,
    mut current_non_leaf_count: &'a mut u16,
    mut child_count: usize,
    wr_config: &W,
    bump: &'b Bump,
    tmp_buf: &mut Bump,
    rng: &mut impl Rng,
    c: f64,
) {
    tmp_buf.reset();
    #[derive(Debug)]
    struct PathData<'r> { score: &'r mut f64, visits: &'r mut u32, chance: &'r mut f64, non_leaf_count_next_state: &'r mut u16, non_leaf_count_current_move: &'r mut u16, child_count: usize }
    let mut el = <W as ExecutionLimiterFactory<G>>::create(wr_config);
    let mut path = bumpalo::collections::Vec::with_capacity_in(30, tmp_buf);
    loop {
        let current = match next {
            MonteCarloChild::Computed(ref mut child) => child,
            MonteCarloChild::Uncomputed(m) => {
                let outcomes = game.get_outcomes(&m).expect("failed to get child");

                let mut outcomes_buf = bumpalo::collections::Vec::new_in(bump);
                outcomes_buf.extend(outcomes.into_iter().map(|(out, chance)| (chance, MonteCarloOutcome::Uncomputed(m.clone(), out))));
                let outcomes = outcomes_buf.into_bump_slice_mut();
                let outcomes_len = outcomes.len() as u16;

                debug_assert!(u16::try_from(outcomes.len()).is_ok());

                let mc_move = MonteCarloMove {
                    outcomes,
                    visits: 0,
                    non_leaf_count: outcomes_len,
                    score: 0.0,
                };
                *next = MonteCarloChild::Computed(mc_move);
                let MonteCarloChild::Computed(ref mut n) = next else { unreachable!() };
                n
            }
        };
        let (chance, outcome) = match select_next_outcome(rng, current.outcomes) {
            None => panic!("{:?}, {:?}", &current.outcomes, current.non_leaf_count),
            Some(i) => &mut current.outcomes[i],
        };
        let mut game_state = GameState::Continue;

        let next_state = match outcome {
            MonteCarloOutcome::Computed(next) => next,
            MonteCarloOutcome::Uncomputed(mov, out) => {
                let result = game.make_move(mov, out).expect("invalid move");
                game_state = result.1;
                let g = bump.alloc(result.0);
                let next_state = MonteCarloState::new(rng, g, game_state == GameState::Finished, bump);
                *outcome = MonteCarloOutcome::Computed(next_state);
                let MonteCarloOutcome::Computed(n) = outcome else { unreachable!() };
                n
            }
        };

        game = next_state.game;

        let parent_visited = current.visits;
        path.push(PathData {
            score: &mut current.score,
            visits: &mut current.visits,
            chance,
            non_leaf_count_next_state: std::mem::replace(&mut current_non_leaf_count, &mut next_state.non_leaf_count),
            child_count: std::mem::replace(&mut child_count, next_state.children.len()),
            non_leaf_count_current_move: &mut current.non_leaf_count,
        });
        if el.next_with_game(next_state.children.len(), game).is_break() {
            return;
        }
        if let GameState::Finished = game_state {
            break;
        }

        let new = select_next_move(
            next_state.children.iter(),
            parent_visited,
            c,
        );
        let new = if let Some(new) = new {
            new
        } else {
            if path.len() == 0 && next_state.children.len() == 0 {
                //weird special case when there is only one playable move
                return;
            }
            panic!("alarm - path: {path:?}");
        };
        let new = &mut next_state.children[new];

        next = new;
    }

    let mut score_reducer = <W as MultiScoreReducerFactory<G>>::create(wr_config, game);
    let mut is_leaf = true;
    for PathData{ score, visits, chance, non_leaf_count_next_state, non_leaf_count_current_move, child_count } in path.into_iter().rev() {
        *score += *chance * score_reducer.next_score(child_count);
        *chance = if is_leaf { 0.0 } else { *chance };
        *non_leaf_count_current_move -= is_leaf as u16;
        is_leaf = *non_leaf_count_current_move == 0;
        *non_leaf_count_next_state -= is_leaf as u16;
        is_leaf = *non_leaf_count_next_state == 0;
        *visits += 1;
    }
}

fn select_next_outcome<T>(
    rng: &mut impl Rng,
    outcomes: &mut [(f64, T)],
) -> Option<usize> {
    let chance_sum = outcomes.iter().map(|(chance, _)| *chance).sum::<f64>();
    if chance_sum == 0.0 {
        return None;
    }
    let the_chance = rng.gen_range(0.0..chance_sum);
    outcomes.iter_mut()
        .enumerate()
        .scan(0.0, |acc, rest| {
            *acc += rest.1.0;
            Some((*acc, rest))
        })
        .find(|(chance, _rest)| the_chance < *chance)
        .map(|(_, (i, _))| i)
}

fn select_next_move<'c, 'b: 'c, G: MonteCarloGameND + 'static>(
    children: impl Iterator<Item=&'c MonteCarloChild<'b, G>>,
    parent_visited: u32, c: f64,
) -> Option<usize> {
    let parent_visited = parent_visited as f64;
    let mut max_i = usize::MAX;
    let mut max_score = f64::NEG_INFINITY;
    let parent_fac = c.powi(2) * parent_visited.max(1.0).ln();
    for (i, child) in children.enumerate() {
        let mov = match child {
            MonteCarloChild::Computed(m) => m,
            MonteCarloChild::Uncomputed(_) => return Some(i),
        };
        let mov_fac = 1.0 / mov.visits.max(1) as f64;
        let score = (mov.score * mov_fac) + (parent_fac * mov_fac).sqrt();
        if score > max_score && mov.non_leaf_count > 0 {
            max_i = i;
            max_score = score;
        };

    }

    if max_i < usize::MAX {
        Some(max_i)
    } else {
        None
    }
}
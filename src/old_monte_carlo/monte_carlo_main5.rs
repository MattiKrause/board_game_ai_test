use std::mem::size_of;
use std::time::{Duration, Instant};
use bumpalo::Bump;
use crate::monte_carlo_game::{MonteCarloGame, Winner};
use crate::{MonteLimit, WinReward};
use crate::ai_infra::GameStrategy;
use crate::monte_carlo_win_reducer::{WinReducer, WinReducerFactory};

#[allow(dead_code)]
pub struct MonteCarloStrategyV5<WRF: WinReducerFactory> {
    limit: MonteLimit, c: f64, wrf: WRF, win_reward: WinReward
}

pub struct MonteCarloCarry {
    allocator: Bump,
}

struct MonteCarloChild<'b, G: MonteCarloGame>(G::MOVE, Option<MonteCarloState<'b, G>>);
enum MonteCarloChildState<'b, G: MonteCarloGame> {
    State(MonteCarloChildState<'b, G>)
}

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

impl<G: MonteCarloGame + 'static, W: WinReducerFactory> GameStrategy<G> for MonteCarloStrategyV5<W> {
    type Carry = MonteCarloCarry;
    type Config = (MonteLimit, f64, W, WinReward);

    fn new((limit, c, wrf, win_reward): (MonteLimit, f64, W, WinReward)) -> Self {
        Self {
            limit,
            c,
            wrf,
            win_reward
        }
    }

    fn make_move(&self, game: &G, carry: Option<(G::MOVE, Self::Carry)>) -> (G::MOVE, Self::Carry) {
        let mut carry = carry.map(|(_, c)| c).unwrap_or_else(|| MonteCarloCarry {
            allocator: Bump::with_capacity(size_of::<G>() * 50_000)
        });
        let m = make_monte_carlo_move(game, &carry.allocator, self.limit, self.c, &self.wrf, self.win_reward);
        carry.allocator.reset();
        (m, carry)
    }
}

pub fn make_monte_carlo_move<G: MonteCarloGame + 'static, W: WinReducerFactory>(g: &G, bump: &Bump, limit: MonteLimit, c: f64, wr_factory: &W, win_reward: WinReward) -> G::MOVE {
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
        let next = select_next(children.iter_mut(), operations, c);
        let next = if let Some(next) = next {
            next
        } else {
            break;
        };
        playoff(g, next, wr_factory, bump, c, win_reward);
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

fn playoff<'a, 'b, G: MonteCarloGame + 'static, W: WinReducerFactory>(
    mut g: &'a G,
    mut next: &'a mut MonteCarloChild<'b, G>,
    wr_config: &W,
    bump: &'b Bump,
    c: f64,
    win_reward: WinReward
) {
    let mut path = Vec::with_capacity(30);
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
        if let Some(w) = current.winner {
            winner = w;
            current.wins += match w {
                Winner::WIN => win_reward.on_win.0,
                Winner::TIE => win_reward.on_tie.0,
            };
            current.leaf_count += 1;
            break;
        }
        g = &current.game;

        let child_count = current.children.len();
        let new = select_next(
            current.children.iter_mut(), current.visited, c,
        ).unwrap();
        path.push((&mut current.wins, &mut current.leaf_count, child_count));
        next = new;
    }

    let (first_score, second_score) = match winner {
        Winner::WIN => win_reward.on_win,
        Winner::TIE => win_reward.on_tie,
    };
    let (mut first_score, mut second_score) = (
        wr_config.create(first_score),
        wr_config.create(second_score)
    );
    let mut inc_first = false;
    let mut is_leaf = true;
    for (wins, leaf_count, child_count) in path.into_iter().rev() {
        *wins += if inc_first {
            second_score.deteriorate();
            first_score.get_and_deteriorate()
        } else {
            first_score.deteriorate();
            second_score.get_and_deteriorate()
        };
        inc_first = !inc_first;
        *leaf_count += is_leaf as u8 as u16;
        is_leaf = *leaf_count as usize >= child_count;
    }
}

fn select_next<'c, 'b, G: MonteCarloGame + 'static>(
    mut children: impl Iterator<Item=&'c mut MonteCarloChild<'b, G>>,
    parent_visited: f64, c: f64,
) -> Option<&'c mut MonteCarloChild<'b, G>> {
    macro_rules! next_eligible_child {
        ($next: ident, $child: ident) => {
            let $child = match $next.1 {
                Some(ref child) => child,
                None => { return Some($next) }
            };
            if $child.visited == 0.0 {
                return Some($next);
            }
            if $child.leaf_count as usize >= $child.children.len() {
                continue;
            }
        };
    }
    let parent_factor = parent_visited.ln() * c;
    let mut max_child;
    let mut max_score;
    loop {
        let next = children.next()?;
        next_eligible_child!(next, child);
        max_score = (child.wins / child.visited) + (parent_factor / child.visited).sqrt();
        max_child = next;
        break;
    }
    while let Some(next) = children.next() {
        next_eligible_child!(next, child);
        let score = (child.wins / child.visited) + (parent_factor / child.visited).sqrt();
        if score > max_score {
            max_child = next;
            max_score = score;
        }
    }
    return Some(max_child);
}
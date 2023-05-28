use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::DerefMut;
use std::rc::Rc;
use std::time::Instant;
use bumpalo::Bump;
use rand::{Rng, SeedableRng};
use rand::seq::SliceRandom;
use rustc_hash::{FxHashMap};
use crate::ai_infra::GameStrategy;
use crate::monte_carlo_game::{MonteCarloGame, Winner};
use crate::monte_carlo_v2::arena::{Arena, ArenaHandle};
use crate::monte_carlo_v2::moves_buffer::{SliceArena, SliceHandle};

type MCNodeId<T> = ArenaHandle<MCNode<T>>;
type Successor<T: MonteCarloGame> = (MCNodeId<T>, T::MOVE);

enum CompactPred<T: MonteCarloGame> {
    LessThanThree([MCNodeId<T>; 2]),
    MoreOrEqThree(Vec<MCNodeId<T>>)
}

struct MCNode<T: MonteCarloGame> {
    predecessors: CompactPred<T>,
    moves: SliceHandle<Successor<T>>,
    game_state: Rc<T>,
    visited_amount: u64,
    score_balance: f64,
    completely_computed: bool
}

pub struct MCContext<T: MonteCarloGame> {
    mappings: FxHashMap<Rc<T>, MCNodeId<T>>,
    node_store: Arena<MCNode<T>>,
    unused_rcs: Vec<Rc<T>>,
    move_store: SliceArena<Successor<T>>,

    tmp_buf: Bump,
    rng: RefCell<rand::rngs::SmallRng>,
}

pub struct MonteCarloV2I4 {
    playoffs: usize,
    rng_seed: Option<[u8; 32]>
}

pub struct MonteCarloConfigV2I4 {
    pub num_playoffs: usize,
    pub rng_seed: Option<[u8; 32]>
}
impl <G: MonteCarloGame> GameStrategy<G> for MonteCarloV2I4 {
    type Carry = MCContext<G>;
    type Config = MonteCarloConfigV2I4;

    fn new(config: Self::Config) -> Self {
        Self {
            playoffs: config.num_playoffs,
            rng_seed: config.rng_seed,
        }
    }

    fn make_move(&self, game: &G, carry: Option<(G::MOVE, Self::Carry)>) -> (G::MOVE, Self::Carry) {
        let rng = self.rng_seed
            .map(|seed| rand::rngs::SmallRng::from_seed(seed))
            .unwrap_or_else(|| rand::rngs::SmallRng::from_entropy());
        let mut context = carry.map(|(_, ctx)| ctx).unwrap_or_else(|| MCContext {
            mappings: HashMap::with_capacity_and_hasher(self.playoffs / 10, Default::default()),
            node_store: Arena::new(),
            unused_rcs: vec![],
            move_store: SliceArena::new(),
            tmp_buf: Default::default(),
            rng: RefCell::new(rng),
        });
        let start = Instant::now();
        let result = (select_move(game, self.playoffs, &mut context), context);
        //1.34836958s
        //1.347581748s
        //1.376205498s
        //1.329065541s
        //1.365577052s
        //1.341316484s
        println!("time taken: {}s", start.elapsed().as_secs_f64());
        result
    }
}

fn select_move<T: MonteCarloGame>(state: &T, times: usize, context: &mut MCContext<T>) -> T::MOVE {
    context.node_store.purge();
    context.move_store.clear();
    context.unused_rcs.reserve(context.mappings.len());
    context.unused_rcs.extend(context.mappings.drain().map(|(state, _)| state));


    let root_node = {
        let game_state = Rc::new(state.clone());
        let moves = game_state.moves().into_iter()
            .map(|mov| (MCNodeId::invalid(), mov));
        let moves = context.move_store.insert(moves);
        let node = MCNode {
            predecessors: CompactPred::LessThanThree([MCNodeId::invalid(); 2]),
            moves,
            game_state,
            visited_amount: 0,
            score_balance: 0.0,
            completely_computed: false,
        };
        context.alloc_node(node)
    };
    let mut buf = Vec::new();
    for _ in 0..times {
        playoff(root_node.clone(), context, 2, &mut buf);
    }
    dbg!(context.node_store.get(&root_node).unwrap().visited_amount);
    let root_node = context.node_store.get(&root_node).unwrap();
    let root_moves = context.move_store.get(&root_node.moves).unwrap();
    root_moves.iter()
        .filter_map(|(id, mov)| context.node_store.get(id).zip(Some(mov)))
        .map(|(node, mov)| (node.score_balance / (node.visited_amount as f64), mov))
        .max_by(|(score1, _), (score2, _)| score1.total_cmp(score2))
        .unwrap()
        .1
        .clone()
}

fn playoff<T: MonteCarloGame + Clone>(root: MCNodeId<T>, context: &mut MCContext<T>, player_count: u8, buf: &mut Vec<(MCNodeId<T>, f64, bool)>) where T: Eq + Hash {
    let mut node = context.node_store.get(&root).expect("root node not given");
    let mut current_id = root;
    let mut current_player_num = 0;
    loop {
        // select next move;

        let moves_ref = context.move_store.get(&node.moves).unwrap();

        context.tmp_buf.reset();
        let next_move_i = if let Some(m) = select_next::<T>(node, moves_ref, context, 2.0) { m } else { break; };
        let next_move = &moves_ref[next_move_i];

        (current_id, node) = if context.node_store.get(&next_move.0).is_some() {
            //Initialised
            let next = context.node_store.get(&next_move.0).unwrap();
            (next_move.0.clone(), next)
        } else {
            //Not Initialised
            let (next_state, winner) = node.game_state.make_move(&next_move.1).unwrap();
            let id = context.mappings.get(&next_state).cloned();

            if matches!(winner, Some(Winner::WIN) if current_player_num == 0) {
                context.node_store.get_mut(&current_id).unwrap().completely_computed = true;
            }

            if let Some(next_id) = id {
                context.node_store.get_mut(&current_id)
                    .and_then(|node| context.move_store.get_mut(&node.moves))
                    .and_then(|moves| moves.get_mut(next_move_i))
                    .unwrap().0 = next_id.clone();

                let next_node = context.node_store.get_mut(&next_id).expect("orphan state-map entry");
                next_node.predecessors.push(current_id.clone());
                let next_node = context.node_store.get(&next_id).unwrap();
                (next_id, next_node)
            } else {
                let game_state = match context.unused_rcs.pop() {
                    None => Rc::new(next_state),
                    Some(mut gs) => {
                        *Rc::get_mut(&mut gs).unwrap() = next_state;
                        gs
                    }
                };
                let next_id = new_node_entry(current_id.clone(), game_state, winner, context);
                context.node_store.get_mut(&current_id)
                    .and_then(|node| context.move_store.get_mut(&node.moves))
                    .and_then(|moves| moves.get_mut(next_move_i))
                    .unwrap().0 = next_id.clone();
                let next_node = context.node_store.get(&next_id).unwrap();
                (next_id, next_node)
            }
        };


        current_player_num = (current_player_num + 1) % player_count;
    }

    backtrack_from_leaf(current_id, context, buf);
}

#[inline(never)]
fn new_node_entry<T: MonteCarloGame>(parent_id: ArenaHandle<MCNode<T>>, game_state: Rc<T>, winner: Option<Winner>, context: &mut MCContext<T>) -> ArenaHandle<MCNode<T>> {
    let (is_leaf, initial_score) = compute_initial_score(winner);
    let moves = if !is_leaf {
        let moves = game_state.moves().into_iter()
            .map(|mov| (MCNodeId::invalid(), mov));
        context.move_store.insert(moves)
    } else {
        SliceHandle::empty()
    };
    let new_node = MCNode {
        predecessors: CompactPred::LessThanThree([parent_id,  MCNodeId::invalid()]),
        moves,
        game_state,
        visited_amount: 1,
        score_balance: initial_score,
        completely_computed: is_leaf,
    };

    let next_id = context.alloc_node(new_node);
    next_id
}

#[inline(never)]
fn select_next<T: MonteCarloGame>(parent: &MCNode<T>, moves: &[(MCNodeId<T>, T::MOVE)], context: &MCContext<T>, c: f64) -> Option<usize> {
    let mut existing = bumpalo::collections::Vec::with_capacity_in(moves.len(), &context.tmp_buf);
    let mut not_existing = bumpalo::collections::Vec::with_capacity_in(moves.len(), &context.tmp_buf);

    for (i,(id, _)) in  moves.iter().enumerate() {
        match context.node_store.get(id) {
            Some(e) => {
                if !e.completely_computed {
                    existing.push(e);
                }
            }
            None => {
                not_existing.push(i)
            }
        }
    }

    if let Some(idx) = not_existing.choose(context.rng.borrow_mut().deref_mut()) {
        return Some(*idx);
    }

    let p_score = c * (parent.visited_amount as f64).ln();
    let mut scores = bumpalo::collections::Vec::with_capacity_in(existing.len(), &context.tmp_buf);
    let mut highest_score = 0.0;
    for node in existing {
        let visited = node.visited_amount as f64;
        let win_score= node.score_balance;
        // may introduce nan if p_score is negative
        let score = (win_score / visited) + (p_score / visited).sqrt();
        let score = if score < 0.0 {
            0.0
        } else {
            score
        };
        highest_score += score;
        debug_assert!(!highest_score.is_nan());
        debug_assert!(highest_score >= 0.0, "highest score {highest_score} is smaller than 0.0 after score {score}");
        scores.push(highest_score)
    }
    debug_assert!(highest_score >= 0.0);

    let rng_value = context.rng.borrow_mut().gen_range(0.0..=highest_score);
    scores.iter().enumerate().find_map(|(i, s)| (*s <= rng_value).then_some(i))
}

fn compute_initial_score(win_state: Option<Winner>) -> (bool, f64) {
    match win_state {
        None => (false, 0.0),
        Some(Winner::TIE) => (true, 0.0),
        Some(Winner::WIN) => (true, 1.0)
    }
}

#[inline(never)]
fn backtrack_from_leaf<T: MonteCarloGame>(leaf: MCNodeId<T>, context: &mut MCContext<T>, buf: &mut Vec<(MCNodeId<T>, f64, bool)>) {
    fn compute_completely_computed<T: MonteCarloGame>(node: &MCNode<T>, context: &MCContext<T>) -> bool {
        if let Some(moves) = context.move_store.get(&node.moves) {
            moves.iter()
                .map(|(id, _)| context.node_store.get(id))
                .all(|node| matches!(node, Some(node) if node.completely_computed))
        } else {
            true
        }
    }
    buf.clear();
    {
        let leaf = context.node_store.get_mut(&leaf).unwrap();
        // queue immediate predecessors
        buf.extend(leaf.predecessors.iter().cloned().map(|pred| (pred, leaf.score_balance, true)));
    };
    let initial_length = buf.len();
    for i in 0..initial_length {
        let (node, score, _) = buf[i].clone();
        let second_level = context.node_store.get(&node).unwrap();
        let new_cc = compute_completely_computed(second_level, context);
        let second_level = context.node_store.get_mut(&node).unwrap();
        second_level.completely_computed |= new_cc;
        second_level.score_balance -= score;
        second_level.visited_amount += 1;
        buf.extend(second_level.predecessors.iter().cloned().map(|pred| (pred, score, second_level.completely_computed)));
    }
    buf.drain(0..initial_length);

    while let Some((next, mut score, check_cc)) = buf.pop() {
        let node = context.node_store.get(&next).unwrap();
        let new_cc = if check_cc {
            compute_completely_computed(node, context)
        } else {
            false
        };
        let node = context.node_store.get_mut(&next).unwrap();
        node.completely_computed |= new_cc;
        score /= node.moves.len() as f64;
        node.score_balance += score;
        node.visited_amount += 1;
        buf.extend(node.predecessors.iter().cloned().map(|pred| (pred, -score, node.completely_computed)))
    }
}

impl <T: MonteCarloGame> CompactPred<T> {
    fn push(&mut self, id: MCNodeId<T>) {
        match self {
            CompactPred::LessThanThree([id0, id1]) => {
                if *id1 == MCNodeId::invalid() {
                    *id1 = id;
                } else {
                    let content = vec![id0.clone(), id1.clone(), id];
                    *self = Self::MoreOrEqThree(content)
                }
            }
            CompactPred::MoreOrEqThree(content) => {
                content.push(id);
            }
        }
    }
    fn iter(&self) -> impl Iterator<Item = &'_ MCNodeId<T>> {
        match self {
            CompactPred::LessThanThree(ids) => {
                let len = if ids[0] == MCNodeId::invalid() {
                    0
                } else if ids[1] == MCNodeId::invalid() {
                    1
                } else {
                    2
                };
                ids[0..len].iter()
            }
            CompactPred::MoreOrEqThree(c) => c.iter()
        }
    }
}

impl<T: MonteCarloGame> MCContext<T> {
    fn alloc_node(&mut self, node: MCNode<T>) -> MCNodeId<T> {
        let node_game = node.game_state.clone();
        let id = self.node_store.insert(node);
        self.mappings.insert(node_game, id.clone());
        id
    }
}
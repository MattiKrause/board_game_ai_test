use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::rc::Rc;
use arrayvec::ArrayVec;
use rustc_hash::{FxHasher, FxHashMap};
use crate::ai_infra::GameStrategy;
use crate::monte_carlo_game::{MonteCarloGame, Winner};
use crate::monte_carlo_v2::arena::{Arena, ArenaHandle};

//#[derive(Clone, Eq, PartialEq, Hash)]
//struct MCNodeId<T: MonteCarloGame>(ArenaHandle<T>);

type MCNodeId<T> = ArenaHandle<MCNode<T>>;

struct NodeSetId(u64);

struct MoveSetId<M>(PhantomData<M>);

enum CompactPred<T: MonteCarloGame> {
    LessThanThree([MCNodeId<T>; 2]),
    MoreOrEqThree(Vec<MCNodeId<T>>)
}

struct MCNode<T: MonteCarloGame> {
    predecessors: CompactPred<T>,
    moves: Box<[(MCNodeId<T>, T::MOVE)]>,
    game_state: Rc<T>,
    visited_amount: u64,
    score_balance: f64,
    completely_computed: bool
}

pub struct MCContext<T: MonteCarloGame> {
    mappings: FxHashMap<Rc<T>, MCNodeId<T>>,
    node_store: Arena<MCNode<T>>,
    unused_rcs: Vec<Rc<T>>
}

/*struct NodeSetStore {
    nodesets: Vec<Vec<(u64, Box<[MCNodeId]>)>>,
}

struct MCNodeStore<T: MonteCarloGame> {
    players: Vec<(u64, Box<ArrayVec<MCNode<T>, 64>>)>,
}

struct MoveSetStore<T: MonteCarloGame> {}

impl NodeSetStore {
    fn construct_id(length: u16, payload: u64) -> NodeSetId {
        NodeSetId(payload << 16 | u64::from(length))
    }
    fn allocate(&mut self, length: usize) -> (NodeSetId, &mut [MCNodeId]) {
        if length == 0 {
            Self::construct_id(0, 0);
        }
        self.ensure_has_capacity(length);
        let set = &mut self.nodesets[length - 1];
        let index = set.iter_mut().enumerate()
            .find_map(|set| (set.1.0.count_zeros() > 0).then_some(set.0))
            .unwrap_or_else(|| {
                set.push((0, vec![MCNodeId::invalid(); length * 64].into_boxed_slice()));
                set.len() - 1
            });
        // using index to avoid lifetime issues
        let (slots, content) = &mut set[index];
        ;
        let slot = slots.leading_ones() as usize;
        *slots |= 1 << slot;
        let node_set_id = Self::construct_id(length as u16, (index * 64 + slot) as u64);
        (node_set_id, &mut content[(slot * length)..(slot * length + length)])
    }
    fn ensure_has_capacity(&mut self, for_length: usize) {
        let add_to_nodesets = for_length.saturating_sub(self.nodesets.len());
        let add = std::iter::repeat_with(Vec::new).take(add_to_nodesets);
        self.nodesets.extend(add);
    }
}

impl<T: MonteCarloGame> MCNodeStore<T> {
    fn allocate(&mut self, init: MCNode<T>) -> MCNodeId {
        let index = self.players.iter_mut().enumerate()
            .find_map(|set| (set.1.0.count_zeros() > 0).then_some(set.0))
            .unwrap_or_else(|| {
                self.players.push((0, Box::new(ArrayVec::new())));
                self.players.len() - 1
            });
        let chunk = &mut self.players[index];
        let slot = chunk.0.leading_ones() as usize;
        if let Some(slot) = chunk.1.get_mut(slot) {
            *slot = init;
        } else {
            chunk.1.push(init);
        }
        chunk.0 |= 1 << slot;
        MCNodeId((index * 64 + slot) as u64)
    }

    fn get_mut(&mut self, id: MCNodeId) -> Option<&mut MCNode<T>> {
        let index = id.0 / 64;
        let slot = id.0 % 64;

        self.players.get_mut(index as usize).and_then(|(slots, content)| if *slots & (1 << slot) > 0 { content.get_mut(slot) } else { None })
    }
}

impl<T: MonteCarloGame> MoveSetStore<T> {
    fn get(&self, id: &MoveSetId<T>) -> Option<&[(MCNodeId, T::MOVE)]> {}
}
*/

impl<T: MonteCarloGame> MCContext<T> {
    fn alloc_node(&mut self, node: MCNode<T>) -> MCNodeId<T> {
        let node_game = node.game_state.clone();
        let id = self.node_store.insert(node);
        self.mappings.insert(node_game, id.clone());
        id
    }
}

fn playoff<T: MonteCarloGame + Clone>(root: MCNodeId<T>, context: &mut MCContext<T>, player_count: u8, buf: &mut Vec<(MCNodeId<T>, f64, bool)>) where T: Eq + Hash {
    let mut node = context.node_store.get(&root).expect("root node not given");
    let mut current_id = root;
    let mut current_player_num = 0;
    loop {
        // select next move;

        let next_move_i = if let Some(m) = select_next::<T>(node, &node.moves, current_player_num == 0, context, 2.0) { m } else { break; };
        let next_move = &node.moves[next_move_i];

        if next_move_i == 7 {
            let x = 1;
        }
//0x55d8ae26c6c0
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
                context.node_store.get_mut(&current_id).and_then(|node| node.moves.get_mut(next_move_i)).unwrap().0 = next_id.clone();

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
                context.node_store.get_mut(&current_id).and_then(|node| node.moves.get_mut(next_move_i)).unwrap().0 = next_id.clone();
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
        game_state.moves().into_iter()
            .map(|mov| (MCNodeId::invalid(), mov))
            .collect::<Vec<_>>()
            .into_boxed_slice()
    } else {
        Box::new([])
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
fn select_next<T: MonteCarloGame>(parent: &MCNode<T>, moves: &[(MCNodeId<T>, T::MOVE)], ai_turn: bool, context: &MCContext<T>, c: f64) -> Option<usize> {
    let mut i_max = usize::MAX;
    let mut max_score = f64::MIN;

    let p_score = c * (parent.visited_amount as f64).ln();
    for (i, (id, _)) in moves.iter().enumerate() {
        let Some(node) = context.node_store.get(id) else { return Some(i) };
        let visited = node.visited_amount as f64;
        let win_score= node.score_balance;
        let score = (win_score / visited) + (p_score / visited).sqrt();
        let cond_neg_inf = f64::from_bits(18442240474082181120 * (node.completely_computed as u64));
        let score = score + cond_neg_inf;
        if score > max_score {
            i_max = i;
            max_score = score;
        }
    }
    return Some(i_max).filter(|i| *i != usize::MAX);
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
        node.moves.iter()
            .map(|(id, _)| context.node_store.get(id))
            .all(|node| matches!(node, Some(node) if node.completely_computed))
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

fn select_move<T: MonteCarloGame>(state: &T, times: usize, context: &mut MCContext<T>) -> T::MOVE {
    context.node_store.purge();

    context.unused_rcs.reserve(context.mappings.len());
    context.unused_rcs.extend(context.mappings.drain().map(|(state, _)| state));


    let root_node = {
        let game_state = Rc::new(state.clone());
        let moves = game_state.moves().into_iter()
            .map(|mov| (MCNodeId::invalid(), mov))
            .collect::<Vec<_>>()
            .into_boxed_slice();
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
    context.node_store.get(&root_node).unwrap().moves.iter()
        .filter_map(|(id, mov)| context.node_store.get(id).zip(Some(mov)))
        .map(|(node, mov)| (node.score_balance / (node.visited_amount as f64), mov))
        .max_by(|(score1, _), (score2, _)| score1.total_cmp(score2))
        .unwrap()
        .1
        .clone()
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

pub struct MonteCarloV2I3 {
    playoffs: usize
}

impl <G: MonteCarloGame> GameStrategy<G> for MonteCarloV2I3 {
    type Carry = MCContext<G>;
    type Config = usize;

    fn new(config: Self::Config) -> Self {
        Self {
            playoffs: config,
        }
    }

    fn make_move(&self, game: &G, carry: Option<(G::MOVE, Self::Carry)>) -> (G::MOVE, Self::Carry) {
        let mut context = carry.map(|(_, ctx)| ctx).unwrap_or_else(|| MCContext {
            mappings: HashMap::with_capacity_and_hasher(self.playoffs / 10, Default::default()),
            node_store: Arena::new(),
            unused_rcs: vec![],
        });
        (select_move(game, self.playoffs, &mut context), context)
    }
}
use std::io::stdin;
use std::mem::replace;
use crate::MonteCarloGame;

pub trait GamePlayer<G: MonteCarloGame> {
    fn make_move(&mut self, game: &G, enemy_move: Option<G::MOVE>) -> G::MOVE;
}

pub trait GameStrategy<G: MonteCarloGame> {
    type Carry;
    type Config;
    fn new(config: Self::Config) -> Self;
    fn strategy_of(config: Self::Config) -> GameStrategyPlayer<G, Self> where Self: Sized{
        GameStrategyPlayer::new(Self::new(config))
    }
    fn make_move(&self, game: &G, carry: Option<(G::MOVE, Self::Carry)>) -> (G::MOVE, Self::Carry);
}

pub struct GameStrategyPlayer<G: MonteCarloGame, GS: GameStrategy<G>> {
    strategy: GS,
    carry: Option<GS::Carry>,
}

impl <G: MonteCarloGame, GS: GameStrategy<G>> GameStrategyPlayer<G, GS>{
    pub fn new(strategy: GS) -> Self {
        Self {
            strategy,
            carry: None,
        }
    }
}

impl <G: MonteCarloGame, GS: GameStrategy<G>> GamePlayer<G> for GameStrategyPlayer<G, GS> {
    fn make_move(&mut self, game: &G, enemy_move: Option<G::MOVE>) -> G::MOVE {
        let carry = enemy_move.zip(replace(&mut self.carry, None));
        let (m, carry) = self.strategy.make_move(game, carry);
        self.carry = Some(carry);
        m
    }
}

pub struct PlayerInput;
impl <G: MonteCarloGame> GamePlayer<G> for PlayerInput where G::MOVE: TryFrom<u32> {
    fn make_move(&mut self, game: &G, _enemy_move: Option<G::MOVE>) -> G::MOVE {
        loop {
            let mut s = String::with_capacity(10);
            println!("enter your turn");
            stdin().read_line(&mut s).expect("Failed to from stdin");
            s = s.trim().to_string();
            let as_num = match s.parse() {
                Err(_) | Ok(0) => {
                println!("cannot parse move num!");
                continue;
                }
                Ok(num) => num,
            };
            let m = match G::MOVE::try_from(as_num - 1) {
                Ok(m) => m,
                Err(_) => {
                    println!("cannot parse move num!");
                    continue;
                }
            };
            let is_valid_move = game.moves().into_iter().any(|it| it == m);
            if !is_valid_move {
                println!("invalid move!");
                continue;
            }
            break m
        }
    }
}
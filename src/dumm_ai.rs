use rand::rngs::SmallRng;
use rand::SeedableRng;
use rand::seq::SliceRandom;
use crate::ai_infra::GameStrategy;
use crate::monte_carlo_game::MonteCarloGame;

pub struct DummAi;


impl <G: MonteCarloGame> GameStrategy<G> for DummAi {
    type Carry = SmallRng;
    type Config = ();

    fn new(_config: Self::Config) -> Self {
        Self
    }

    fn make_move(&self, game: &G, carry: Option<(G::MOVE, Self::Carry)>) -> (G::MOVE, Self::Carry) {
        let mut rng = carry.map(|(_, rng)| rng).unwrap_or(SmallRng::from_entropy());
        let moves = game.moves().into_iter().map(|m| (game.make_move(&m).unwrap(), m)).collect::<Vec<_>>();
        for ((_, res), m) in &moves{
            if res.is_some() {
                return (m.clone(), rng)
            }
        }
        let viable_moves = moves.iter()
            .map(|((game,_), m)| (game.moves().into_iter().map(|m| game.make_move(&m).unwrap()), m))
            .filter_map(|(mut result, m)| result.all(|(_, res)| res.is_none()).then_some(m))
            .collect::<Vec<_>>();
        let mov = if !viable_moves.is_empty() {
            viable_moves.choose(&mut rng).map(|m| (*m).clone()).unwrap()
        } else {
            moves.choose(&mut rng).map(|(_, m)| m).cloned().unwrap()
        };
        (mov, rng)
    }
}
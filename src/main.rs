use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use crate::ai_infra::*;
use crate::line_four_8x8::{LineFour8x8, LineFour8x8Index};
use crate::monte_carlo_game::{MonteCarloGame, TwoPlayer, Winner};
use old_monte_carlo::monte_carlo_main::*;
use old_monte_carlo::monte_carlo_main3::*;
use old_monte_carlo::monte_carlo_main4::MonteCarloStrategyV4;
use crate::monte_carlo_v2::{MonteCarloV2I1, MonteCarloV2I2};
use crate::monte_carlo_win_reducer::{WinFactorReduceFactory, WinIdentFactory};

mod line_four_7x6;
mod monte_carlo_game;
mod ai_infra;
mod monte_carlo_win_reducer;
mod line_four_8x8;
mod old_monte_carlo;
mod monte_carlo_v2;

fn main() {

    println!("Hello, world!");
    let half_wr = WinIdentFactory;
    //let win_reward1 = WinReward::new(0.5, 1.0, -1.1);
    let win_reward2 = WinReward::new(0.0, 1.0, -1.0);
    let config: [Box<dyn GamePlayer<_>>; 2] = [
        //Box::new(MonteCarloStrategyV4::strategy_of((MonteLimit::duration(1000),0.5, half_wr, win_reward2))),
        Box::new(MonteCarloStrategyV3::strategy_of((MonteLimit::times(100000),0.5, half_wr, win_reward2))),
        Box::new(MonteCarloV2I2::strategy_of(100000)),
        //Box::new(PlayerInput),
        //Box::new(RecordedMoves(vec![LineFour8x8Index::I3, LineFour8x8Index::I3, LineFour8x8Index::I5, LineFour8x8Index::I3]))
    ];
    run_game::<LineFour8x8>(config);
}

fn run_games<G: MonteCarloGame + 'static, F: FnMut() -> [Box<dyn GamePlayer<G>>; 2]>(times: u32, mut config: F) {
    let mut p1_win = 0u32;
    let mut p2_win = 032;
    let mut tie = 0u32;
    for i in 0..times {
        let mut config = config();
        let swap = i % 2 != 0;
        if swap {
            config.swap(0, 1);
        }
        let (winner, game) = run_game(config);
        match winner {
            Winner::WIN => {
                let mut player = game.player();
                if swap { player = player.next(); }
                match player {
                    TwoPlayer::P1 => p1_win += 1,
                    TwoPlayer::P2 => p2_win += 1,
                }
            }
            Winner::TIE => tie += 1,
        }
    }
    let times = f64::from(times);
    println!("p1_rate: {}, p2_rate: {}, tie_rate: {}", f64::from(p1_win) / times, f64::from(p2_win) / times, f64::from(tie) / times);
}

fn run_game<G: MonteCarloGame + 'static>(mut config: [Box<dyn GamePlayer<G>>; 2]) -> (Winner, G) {
    let mut game = G::new();
    println!("{game:?}");
    let mut last_move = None;
    loop {
        let config = match game.player() {
            TwoPlayer::P1 => &mut config[0],
            TwoPlayer::P2 => &mut config[1],
        };
        let m = config.make_move(&game, last_move);
        let (new_game, winner) = game.make_move(&m)
            .expect("could not make move");
        game = new_game;
        last_move = Some(m);
        println!("{game:?}");
        if let Some(winner) = winner {
            match winner {
                Winner::WIN => println!("{:?} has won", game.player()),
                Winner::TIE => println!("TIE!")
            }
            break (winner, game);
        }
    }
}

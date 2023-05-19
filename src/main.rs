extern crate core;

use std::collections::{HashMap, HashSet};
use std::num::NonZeroU64;
use std::rc::Rc;
use crate::ai_infra::*;
use crate::line_four_8x8::{LineFour8x8, LineFour8x8Index};
use crate::monte_carlo_game::{MonteCarloGame, TwoPlayer, Winner};
use old_monte_carlo::monte_carlo_main::*;
use old_monte_carlo::monte_carlo_main3::*;
use old_monte_carlo::monte_carlo_main4::MonteCarloStrategyV4;
use crate::monte_carlo_v2::{MonteCarloConfigV2I4, MonteCarloV2I1, MonteCarloV2I2, MonteCarloV2I3, MonteCarloV2I4};
use crate::monte_carlo_win_reducer::{ScoreAveragerFactory, WinFactorReduceFactory, WinIdentFactory};
use crate::multi_score_reducer::{TwoScoreReducerExecutionLimiterFactory, TwoScoreReducerFactory, WinRewardInit};
use crate::old_monte_carlo::monte_carlo_main5::MonteCarloStrategyV5;
use crate::old_monte_carlo::monte_carlo_main6::MonteCarloStrategyV6;
use crate::old_monte_carlo::monte_carlo_main7::MonteCarloStrategyV7;
use crate::tic_tac_toe::TicTacToe;

mod line_four_7x6;
mod monte_carlo_game;
mod ai_infra;
mod monte_carlo_win_reducer;
mod line_four_8x8;
mod old_monte_carlo;
mod monte_carlo_v2;
mod multi_score_reducer;
mod tic_tac_toe;

fn main() {

    println!("Hello, world!");
    run_games::<LineFour8x8,  _>(15, || {
        let half_wr = ScoreAveragerFactory;
        let long_view_eval = WinFactorReduceFactory { by: 0.5 };
        let score_reducer = TwoScoreReducerFactory::new(
            WinRewardInit::new(100.0, 50.0, half_wr),
            WinRewardInit::new(-150.0, 50.0, half_wr),
        );

        let score_reducer2 = TwoScoreReducerFactory::new(
            WinRewardInit::new(10.0, 5.0, long_view_eval),
            WinRewardInit::new(-15.0, 5.0, long_view_eval)
        );

        let trs = score_reducer2.limiter_from(0.0001);


        let config: [Box<dyn GamePlayer<_>>; 2] = [
            //Box::new(MonteCarloStrategyV4::strategy_of((MonteLimit::duration(1000),0.5, half_wr, win_reward1))),
            //Box::new(MonteCarloStrategyV3::strategy_of((MonteLimit::times(100000),0.5, half_wr, win_reward2))),
            //Box::new(MonteCarloStrategyV5::strategy_of((MonteLimit::Duration { millis: NonZeroU64::new(2000).unwrap() }, std::f64::consts::SQRT_2, half_wr, win_reward2, None))),
            //Box::new(MonteCarloStrategyV6::strategy_of((MonteLimit::duration(1000), 1.0, score_reducer.clone(), None))),
            Box::new(MonteCarloStrategyV1::strategy_of((MonteLimit::duration(100), 1.0))),
            //Box::new(MonteCarloStrategyV6::strategy_of((MonteLimit::duration(100), 1.0, score_reducer, None))),
            Box::new(MonteCarloStrategyV7::strategy_of((MonteLimit::duration(100), 1.0, trs, None))),
            //Box::new(PlayerInput)
            //Box::new(RecordedMoves(vec![LineFour8x8Index::I3, LineFour8x8Index::I3, LineFour8x8Index::I5, LineFour8x8Index::I3]))
        ];
        config
    });
}

fn run_games<G: MonteCarloGame + 'static, F: FnMut() -> [Box<dyn GamePlayer<G>>; 2]>(times: u32, mut config: F) {
    let mut p1_win = 0u32;
    let mut p2_win = 0u32;
    let mut tie = 0u32;

    //is swapped immediately
    let mut p1_win_ref = &mut p2_win;
    let mut p2_win_ref = &mut p1_win;
    for i in 0..times {
        println!("game: {i}");
        let mut config = config();
        let swap = i % 2 != 0;
        (p1_win_ref, p2_win_ref) = (p2_win_ref, p1_win_ref);
        if swap {
            config.swap(0, 1);
        }
        let (winner, game) = run_game(config, true);
        match winner {
            Winner::WIN => {
                let mut player = game.player();
                match player {
                    TwoPlayer::P1 => *p1_win_ref += 1,
                    TwoPlayer::P2 => *p2_win_ref += 1,
                }
            }
            Winner::TIE => tie += 1,
        }
    }
    assert!(p1_win <= times);
    assert!(p2_win <= times);
    let times = f64::from(times);
    println!("p1_rate: {}, p2_rate: {}, tie_rate: {}", f64::from(p1_win) / times, f64::from(p2_win) / times, f64::from(tie) / times);
}

fn run_game<G: MonteCarloGame + 'static>(mut config: [Box<dyn GamePlayer<G>>; 2], should_print: bool) -> (Winner, G) {
    macro_rules! cprintln {
        ($lit: literal $(, $e: expr)*) => {if should_print { println!($lit $(, $e)*) }};
    }
    let mut game = G::new();
    cprintln!("{game:?}");
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
        cprintln!("{game:?}");
        if let Some(winner) = winner {
            match winner {
                Winner::WIN => cprintln!("{:?} has won", game.player()),
                Winner::TIE => cprintln!("TIE!")
            }
            break (winner, game);
        }
    }
}

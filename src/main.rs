extern crate core;


use log::LevelFilter;
use old_monte_carlo::monte_carlo_main::*;
use old_monte_carlo::monte_carlo_main3::*;


use crate::ai_infra::*;
use crate::dumm_ai::DummAi;
use crate::genetic_algo_op::opt;
use crate::line_four_8x8::{LineFour8x8};
use crate::monte_carlo_game::{MonteCarloGame, TwoPlayer, Winner};

use crate::monte_carlo_win_reducer::{ScoreAveragerFactory, WinFactorReduceFactory, WinIdentFactory};
use crate::multi_score_reducer::{TwoScoreReducerFactory, WinRewardInit};



use crate::old_monte_carlo::monte_carlo_main8::MonteCarloStrategyV8;


mod line_four_7x6;
mod monte_carlo_game;
mod ai_infra;
mod monte_carlo_win_reducer;
mod line_four_8x8;
mod old_monte_carlo;
mod monte_carlo_v2;
mod multi_score_reducer;
mod tic_tac_toe;
mod monte_carlo_game_v2;
mod dumm_ai;
mod genetic_algo_op;

fn main() {
    println!("Hello, world!");
    env_logger::builder().filter_level(LevelFilter::Info).init();
    //rayon::ThreadPoolBuilder::new().num_threads(4).build_global().expect("failed to build thread pool");
    //opt::<LineFour8x8>();

    run_games::<LineFour8x8,  _>(15, || {
        let long_view_eval = WinFactorReduceFactory { by: 0.5 };
        let score_reducer1 = TwoScoreReducerFactory::new(
            WinRewardInit::new
                (-1.5, 5.0, long_view_eval),
            WinRewardInit::new(1.0, 5.0, long_view_eval),
        );

        let score_reducer2 =TwoScoreReducerFactory::new(
            WinRewardInit::new(1.0, 5.0, long_view_eval),
            WinRewardInit::new(-1.5, 5.0, long_view_eval),
        );

        let trs1 = score_reducer1.limiter_from(0.0001);
        let _trs2 = score_reducer2.limiter_from(0.0001);

        //let best_ai = genetic_algo_op::load_best_from_pop::<LineFour8x8>(MonteLimit::duration(100)).expect("no impl");

        let config: [Box<dyn GamePlayer<_>>; 2] = [
            //Box::new(MonteCarloStrategyV4::strategy_of((MonteLimit::duration(1000),0.5, half_wr, win_reward1))),
            //Box::new(MonteCarloStrategyV3::strategy_of((MonteLimit::times(100000),0.5, half_wr, win_reward2))),
            //Box::new(MonteCarloStrategyV5::strategy_of((MonteLimit::Duration { millis: NonZeroU64::new(2000).unwrap() }, std::f64::consts::SQRT_2, half_wr, win_reward2, None))),
            //Box::new(MonteCarloStrategyV6::strategy_of((MonteLimit::duration(1000), 1.0, score_reducer.clone(), None))),
            Box::new(DummAi::strategy_of(())),
            Box::new(MonteCarloStrategyV8::strategy_of((MonteLimit::duration(100), 1.0, trs1, None))),
            //Box::new(MonteCarloStrategyV6::strategy_of((MonteLimit::duration(100), 1.0, score_reducer, None))),
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
                let player = game.player();
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

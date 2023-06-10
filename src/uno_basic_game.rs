use std::fmt::Formatter;
use std::ops::{BitOr, Mul};
use log::debug;
use crate::monte_carlo_game::{GameWithMoves, MonteCarloGame, TwoPlayer, Winner};
use crate::monte_carlo_game_v2::GameState;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum CardRepr {
    Colored(CardColor, ColoredCardKind),
    Special(SpecialCardKind)
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum CardColor {
    Red = 0, Blue = 1, Green = 2, Yellow = 3
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum ColoredCardKind {
    Number(NumberCardKind),
    Effect(EffectCardKind)
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum NumberCardKind {
    Zero = 0, One = 1, Two = 2, Three = 3, Four = 4, Five = 5, Six = 6, Seven = 7, Eight = 8, Nine = 9
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum EffectCardKind {
    Skip, Reverse, DrawTwo, ChosenColor
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum SpecialCardKind {
    DrawFour, ChooseColor
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum CurrentPlayer {
    One = 0, Two = 1, Three = 2, Four = 3
}

#[repr(u8)]
enum PlayerAmount {
    Two = 2, Three = 3, For = 4
}

// bit 0-1: Color
// bit 2-6: Kind - in decimal 0-9 numbers, 10 reverse direction, 11 skip, 12: draw two cards, 13: choosen color, 14: black choose color, 15: black draw 4 cards,

// bits: 7(= bits per card) * 108(= card amount) + 2(= player count) + 2(= current player) + 1(= player move_direction) + 4(= max player count) * 6(= max amount of cards) + 6(= draw stack dist), 4(= carry dist)
struct Uno {
    meta_data: UnoMetadata,
    cards: [u8; 108],
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct UnoMetadata(u64);// 0-1 player count, 2-3 current player, 4 next player direction, 5-10 11-16 17-22 23-27 the player card offset, 28-33 the draw stack offset, 34 - 37 draw cards carry, 38-63: rng seed

#[derive(Copy, Clone, Debug)]
enum UnoMoveErr {
    CardCannotBePlaced, SelectedCardNotInHand, ColorChoosingRequired, ColorChoosingNotRequired, NothingNotNecessary
}

static INITIAL_CARDS: [u8; 108] = initial_cards();


const PLAYER_COUNT_OFF: u64 = 0;
const CURRENT_PLAYER_OFF: u64 = 2;
const NEXT_PLAYER_DIRECTION_OFF: u64 = 4;
const PLAYER_CARD_OFFSET_OFF: u64 = 5;
const DRAW_STACK_OFFSET_OFF: u64 = 29;
const DRAW_CARDS_CARRY_OFF: u64 = 34;
const SEED_OFF: u64 = 38;

const UNO_CARD_REVERSE: u8 = 10;
const UNO_CARD_SKIP: u8 = 11;
const UNO_CARD_DRAW_TWO: u8 = 12;
const UNO_CARD_CHOOSE_COLOR_COLORED: u8 = 13;
const UNO_CARD_CHOOSE_COLOR_BLACK: u8 = 14;
const UNO_CARD_DRAW_FOUR: u8 = 15;
const UNO_CARD_SMALLEST_BLACK: u8 = 14;

const BACK_STACK: usize = 1;
const OPEN_CARD_IDX: usize = 0;

const UNO_CARD_COLOR_MASK: u8 = 0b11;
const UNO_CARD_KIND_OFF: u8 = 2;
const UNO_CARD_COLOR_OFF: u8 = 0;

const CARD_OFFSET_MASK: u64 = 0b111_111;
const DRAW_CARD_CARRY_MASK: u64 = 0b1111;

const fn initial_cards() -> [u8; 108] {
    let mut accum = [0u8; 108];
    let mut accum_index = 0;

    macro_rules! wa {
        ($v: expr) => {accum[accum_index] = $v; accum_index += 1;};
    }

    let mut color = 0;
    while color < 4 {
        let color = {
            let c_ = color;
            color += 1;
            c_
        };
        wa!(color);//zero cards(only one is inserted)
        wa!(13 >> 2);
        wa!(14 >> 2);

        let mut kind = 0;

        while kind < 4 {
            let kind = {
                let k_ = kind;
                kind += 1;
                k_
            };

            wa!(kind >> 2);
        }
    }

    accum
}



impl Uno {
    fn new(seed: u32,  player_count: PlayerAmount) -> Self {
        let seed = seed & (u32::MAX >> (64 - SEED_OFF as u32));

        let mut cards = INITIAL_CARDS;
        let mut running_seed = seed;
        for i in (1..cards.len()).rev() {
            let idx = generate_random_num(&mut running_seed) as usize % i;
            cards.swap(i, idx);
        }

        let player_count = player_count as u64 - 2;
        let current_player = 0;
        let mut player_offs = [0, 7, 14, 14];
        if player_count == 1 {
            player_offs[3] += 7;
        }
        player_offs.iter_mut().for_each(|o| *o += 1);

        let draw_stack_off = player_offs[3] + 7;

        let player_off_bits = player_offs.into_iter().enumerate().map(|(i, p)| p << i as u64 * 6).fold(0,u64::bitor);
        let meta_data = (player_count << PLAYER_COUNT_OFF) | (current_player << CURRENT_PLAYER_OFF) | (1 << NEXT_PLAYER_DIRECTION_OFF) | (player_off_bits << PLAYER_CARD_OFFSET_OFF) | (draw_stack_off << DRAW_STACK_OFFSET_OFF) | ((seed as u64) << SEED_OFF);

        Self {
            meta_data: UnoMetadata(meta_data),
            cards,
        }
    }

    fn get_open_card(&self) -> u8 {
        self.cards[OPEN_CARD_IDX]
    }

    fn get_p_cards(&self, p: u64) -> Option<impl Iterator<Item = u8> + '_> {
        let player_count = self.meta_data.get_player_count() + 2;
        if p >= player_count {
            return None;
        }
        let start = self.meta_data.get_current_card_offset(p) as usize;
        let end = self.meta_data.get_next_card_offset(p) as usize;
        Some(self.cards[start..end].iter().copied())
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct UnoMove(u8);

impl std::fmt::Debug for UnoMove {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        UnoMoveEnum::from(*self).fmt(f)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum UnoMoveEnum {
    ChooseCard(u8),
    ChooseColor(u8),
    Nothing
}

impl From<UnoMoveEnum> for UnoMove {
    fn from(value: UnoMoveEnum) -> Self {
        match value {
            UnoMoveEnum::ChooseCard(c) => UnoMove(c),
            UnoMoveEnum::ChooseColor(c) => UnoMove(c + 108),
            UnoMoveEnum::Nothing => UnoMove(112)
        }
    }
}

impl From<UnoMove> for UnoMoveEnum {
    fn from(value: UnoMove) -> Self {
        if value.0 < 108 {
            UnoMoveEnum::ChooseCard(value.0)
        } else if value.0 < 112 {
            UnoMoveEnum::ChooseColor(value.0 - 108)
        } else {
            UnoMoveEnum::Nothing
        }
    }
}


impl GameWithMoves for Uno {
    type Move = UnoMove;
    type MoveErr = UnoMoveErr;

    fn execute_move(&mut self, m: &Self::Move) -> Result<GameState, UnoMoveErr> {

         match UnoMoveEnum::from(*m) {
             UnoMoveEnum::ChooseCard(card_idx) => {
                 let current_player = self.meta_data.get_current_player();

                 let card_idx = {
                     let card_idx = card_idx as usize;
                     let current_offset = self.meta_data.get_current_card_offset(current_player) as usize;
                     let next_offset = self.meta_data.get_next_card_offset(current_player) as usize;
                     if !(card_idx + current_offset < next_offset) {
                         return Err(UnoMoveErr::SelectedCardNotInHand)
                     }
                     card_idx + current_offset
                 };

                 let open_card = self.get_open_card();
                 if open_card >> UNO_CARD_KIND_OFF == UNO_CARD_CHOOSE_COLOR_BLACK {
                     return Err(UnoMoveErr::ColorChoosingRequired)
                 }

                 let selected_card = self.cards[card_idx];
                 let selected_card_kind = selected_card >> UNO_CARD_KIND_OFF;

                 if !can_first_be_put_onto_second(selected_card, open_card) {
                     return Err(UnoMoveErr::CardCannotBePlaced)
                 }

                 {
                     let player_card_start = self.meta_data.get_index_after_discard_stack() as usize;
                     self.cards.copy_within(player_card_start..card_idx, player_card_start + 1);
                     self.cards[player_card_start] = post_process_open_card(open_card);
                     self.cards[OPEN_CARD_IDX] = selected_card;
                 }

                 self.meta_data.add_to_all_offsets_starting_at(0, 1);



                 if is_draw_card(open_card) && !is_draw_card(selected_card) {
                     let next_offset = self.meta_data.get_next_card_offset(current_player);
                     let draw_amount = (self.meta_data.get_and_zero_draw_card_carry() + 2) as usize;
                     let draw_stack_offset = self.meta_data.get_draw_stack_offset();
                     let draw_stack_len = 108 - draw_stack_offset as usize;
                     let negative_shift;

                     if draw_amount > draw_stack_len {
                         let discard_stack_end = self.meta_data.get_index_after_discard_stack();
                         rotate_by_reverse(&mut self.cards[1..], discard_stack_end as usize - 1);
                         negative_shift = discard_stack_end - 1;
                     } else {
                         negative_shift = 0;
                     }

                     let draw_amount = draw_amount.min(draw_stack_len + negative_shift as usize);
                     rotate_by(&mut self.cards[((next_offset - negative_shift) as usize)..], draw_amount);

                     self.meta_data.subtract_from_all_offsets(negative_shift);
                     self.meta_data.add_to_all_offsets_after(current_player, draw_amount as u64);
                 }

                 {
                     let mut add_to_carry: u64 = if selected_card_kind == UNO_CARD_DRAW_TWO { 2 } else if selected_card_kind == UNO_CARD_DRAW_FOUR  { 4 } else { 0 };
                     if is_draw_card(open_card) {
                         add_to_carry = add_to_carry.saturating_sub(2)
                     }
                     self.meta_data.add_to_card_draw_carry(add_to_carry);
                 }

                 self.meta_data.switch_player_direction_if(selected_card_kind == UNO_CARD_REVERSE);

                 if self.meta_data.get_current_card_offset(current_player) == self.meta_data.get_next_card_offset(current_player) {
                     return Ok(GameState::Finished)
                 }

                 {
                     let advance_by = 1 + (selected_card_kind == UNO_CARD_SKIP) as u64 - (selected_card_kind == UNO_CARD_CHOOSE_COLOR_BLACK) as u64;
                     self.meta_data.compute_and_set_next_player(advance_by);
                 }


                 Ok(GameState::Continue)
             }
             UnoMoveEnum::ChooseColor(c) => {
                 debug_assert!(c <= 3);
                 if (self.cards[OPEN_CARD_IDX] >> UNO_CARD_KIND_OFF) == UNO_CARD_CHOOSE_COLOR_BLACK {
                     self.cards[0] = (c << UNO_CARD_COLOR_OFF) | (UNO_CARD_CHOOSE_COLOR_COLORED << UNO_CARD_KIND_OFF);
                     self.meta_data.compute_and_set_next_player(1);
                     Ok(GameState::Continue)
                 } else {
                     return Err(UnoMoveErr::ColorChoosingNotRequired)
                 }
             }
             UnoMoveEnum::Nothing => {
                 let open_card = self.get_open_card();
                 if open_card >> UNO_CARD_KIND_OFF == UNO_CARD_CHOOSE_COLOR_BLACK {
                     return Err(UnoMoveErr::ColorChoosingRequired)
                 }

                 let current_player = self.meta_data.get_current_player();
                 let current_offset = self.meta_data.get_current_card_offset(current_player) as usize;
                 let next_offset = self.meta_data.get_next_card_offset(current_player) as usize;

                 let has_viable_card = self.cards[current_offset..next_offset].iter().any(|card| can_first_be_put_onto_second(*card, open_card));

                 if has_viable_card {
                     return Err(UnoMoveErr::NothingNotNecessary)
                 }

                 let draw_stack_offset = self.meta_data.get_draw_stack_offset() as usize;
                 let viable_card = self.cards[draw_stack_offset..].iter().enumerate().find(|(_, card)| can_first_be_put_onto_second(**card, open_card));

                 match viable_card {
                     Some((i, _)) => {
                         let drawn_cards = (i + 1) - draw_stack_offset;
                         rotate_by(&mut self.cards[next_offset..=i], drawn_cards);
                         self.meta_data.add_to_all_offsets_after(current_player, drawn_cards as u64);
                     }
                     None => {
                         randomise_discard_stack(self);

                         let discard_stack_end = self.meta_data.get_index_after_discard_stack() as usize;
                         let discard_stack = &mut self.cards[1..discard_stack_end];

                         let rotate_into_player_stack;
                         let skip_player;
                         match discard_stack.iter().enumerate().find(|(_, card)| can_first_be_put_onto_second(**card, open_card)) {
                             None => {
                                 rotate_into_player_stack = discard_stack.len();
                                 skip_player = true;
                                 // put discard, draw stack on player, skip player

                             }
                             Some((i, _)) => {
                                 rotate_into_player_stack = i + 1;
                                 skip_player = false;
                                 // put until i on player stack,
                             }
                         }

                         let draw_stack_len = 108 - draw_stack_offset;

                         rotate_by_reverse(&mut self.cards[1..next_offset], rotate_into_player_stack);
                         rotate_by(&mut self.cards[next_offset..], draw_stack_len);
                         rotate_by_reverse(&mut self.cards[1..], discard_stack_end - 1 - rotate_into_player_stack);

                         self.meta_data.subtract_from_all_offsets(discard_stack_end as u64 - 1);
                         self.meta_data.add_to_all_offsets_after(current_player, (draw_stack_len + rotate_into_player_stack) as u64 - 1);

                         if !skip_player {
                             self.meta_data.compute_and_set_next_player(1);
                         }
                     }
                 }
                 Ok(GameState::Continue)
             }
         }
    }
}

impl std::fmt::Debug for UnoMetadata {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnoMetadata")
            .field("player_count", &self.get_player_count())
            .field("current_player", &self.get_current_player())
            .field("next_player_direction", &self.get_signed_next_player())
            .field("draw_card_carry", &((self.0 >> DRAW_CARDS_CARRY_OFF) & DRAW_CARD_CARRY_MASK))
            .field("player_card_offsets", &&self.get_all_offsets()[0..4])
            .field("draw_stack_offset", &self.get_all_offsets()[4])
            .finish()
    }
}

impl UnoMetadata {
    fn get_current_player(&self) -> u64 {
        (self.0 >> CURRENT_PLAYER_OFF) & 0b11
    }

    fn _get_card_offset(&self, offset_of: u64) -> u64 {
        debug_assert!(offset_of < 5);
        let card_offset = (self.0 >> (PLAYER_CARD_OFFSET_OFF + 6 * offset_of)) & CARD_OFFSET_MASK;
        debug_assert!(card_offset <= 108);
        card_offset
    }

    fn get_current_card_offset(&self, current: u64) -> u64 {
        debug_assert!(current <= 3);
        self._get_card_offset(current)
    }
    fn get_next_card_offset(&self, current: u64) -> u64 {
        debug_assert!(current <= 3);
        self._get_card_offset(current + 1)
    }

    fn get_draw_stack_offset(&self) -> u64 {
        self._get_card_offset(4)
    }

    fn get_index_after_discard_stack(&self) -> u64 {
        self._get_card_offset(0)
    }

    fn get_draw_card_carry(&self) -> u64 {
        (self.0 >> DRAW_CARDS_CARRY_OFF) & DRAW_CARD_CARRY_MASK
    }

    fn get_signed_next_player(&self) -> i64 {
        get_signed_direction((self.0 >> NEXT_PLAYER_DIRECTION_OFF) & 0b1)
    }

    fn get_player_count(&self) -> u64 {
        let pc_raw = (self.0 >> PLAYER_COUNT_OFF) & 0b11;
        let player_count = pc_raw + 2;
        debug_assert!(player_count <= 4);
        return player_count;
    }

    fn get_all_offsets(&self) -> [u64; 5] {
        let offsets = std::array::from_fn(|i| i as u64 * 6).map(|o| ((self.0 >> PLAYER_CARD_OFFSET_OFF) >> o) & CARD_OFFSET_MASK);

        debug_assert!(offsets.iter().all(|offset| *offset <= 108));
        offsets
    }

    fn get_and_zero_draw_card_carry(&mut self) -> u64 {
        let draw_carry = self.get_draw_card_carry();
        self.0 ^= draw_carry << DRAW_CARDS_CARRY_OFF;
        draw_carry
    }

    fn switch_player_direction_if(&mut self, switch: bool) {
        self.0 ^= (switch as u64) << NEXT_PLAYER_DIRECTION_OFF;
    }

    fn add_to_card_draw_carry(&mut self, value: u64) {
        debug_assert!(self.get_draw_card_carry().saturating_add(value) <= DRAW_CARD_CARRY_MASK);
        self.0 += value << DRAW_CARDS_CARRY_OFF;
    }

    fn compute_and_set_next_player(&mut self, advance_by: u64) {
        let current_player = self.get_current_player();
        let player_count = self.get_player_count();
        debug_assert!(advance_by <= player_count);
        let signed_direction = self.get_signed_next_player() * (advance_by as i64);
        let next_player = next_player(player_count, current_player, signed_direction);
        self.0 ^= (current_player ^ next_player) << CURRENT_PLAYER_OFF;
        debug_assert!(self.get_current_player() == next_player);
    }

    fn subtract_from_all_offsets(&mut self, value: u64) {
        let prev_offsets = self.get_all_offsets();
        let old_metadata = self.0;
        debug_assert!(prev_offsets.into_iter().all(|offset| offset >= value));

        let sub = (0..5).fold(0, |acc, value| (acc << 6) | value);
        self.0 -= sub;

        let new_offsets = self.get_all_offsets();
        debug_assert_eq!((self.0 ^ old_metadata) & (!(u64::MAX << (6 * 5)) << PLAYER_CARD_OFFSET_OFF), 0);
        debug_assert!((0..5).map(|i| (prev_offsets[i], new_offsets[i])).all(|(old, new)| new == old - value))
    }

    fn add_to_all_offsets_after(&mut self, player: u64, value: u64) {
        debug_assert!(player <= 4);
        self.add_to_all_offsets_starting_at(player + 1, value)
    }

    fn add_to_all_offsets_starting_at(&mut self, player: u64, value: u64) {
        debug_assert!(player <= 4);
        let prev_offsets = self.get_all_offsets();
        let old_metadata = self.clone();
        let offsets_bitdata = (self.0 >> PLAYER_CARD_OFFSET_OFF) & !(u64::MAX << (5 * 6));
        println!("alldata{:30b}", !(u64::MAX << (5 * 6)));
        println!("bitdata{:30b}", offsets_bitdata);
        debug_assert!(prev_offsets.iter().copied().all(|offset| offset + value <= 108));
        let add = (0..5).map(|o| value << (o * 6)).fold(0, u64::bitor);
        let add_after_mask = u64::MAX << (6 * player);
        println!("adddata{:30b}", (add & add_after_mask));
        println!("resdata{:30b}", add + offsets_bitdata);
        self.0 += (add & add_after_mask) << PLAYER_CARD_OFFSET_OFF;

        let new_offsets = self.get_all_offsets();

        dbg!(&self, old_metadata, player, value);
        let m1 = !(u64::MAX << (5 * 6));
        let m2 = !(m1 << PLAYER_CARD_OFFSET_OFF);
        debug_assert_eq!((self.0 ^ old_metadata.0) & m2, 0);

        debug_assert!((0..(player as usize)).map(|i| (prev_offsets[i], new_offsets[i])).all(|(old, new)| old == new));
        debug_assert!(((player as usize + 1)..5).map(|i| (prev_offsets[i], new_offsets[i])).all(|(old, new)| new == old + value));
    }
}

fn randomise_discard_stack(uno: &mut Uno) {

    let seed = uno.meta_data.0 ^ uno.cards.iter().fold(0u64, |a, b| a.rotate_left(8) ^ (*b as u64));
    let mut seed = (seed as u32).mul((seed >> 32) as u32);

    let discard_stack_end = uno.meta_data.get_index_after_discard_stack();
    let discard_stack_end = discard_stack_end as usize;
    let discard_stack = &mut uno.cards[1..discard_stack_end];
    for i in 1..discard_stack.len() {
        let j = generate_random_num(&mut seed) as usize;
        uno.cards.swap(i, j);
    }
}

fn rotate_by(mem: &mut [u8], mut by: usize) {
    if mem.len() <= by && by >= 1 {
        return
    }

    for _ in 0..(by / 32) {
        rotate_by_fixed(mem, 32);
    }

    if by % 32 > 0 {
        rotate_by_fixed(mem, by % 32)
    }
}

fn rotate_by_fixed(mem: &mut [u8], by: usize) {
    assert!(by <= 32);
    assert!(mem.len() >= by);
    let mem_len = mem.len();
    let mut buf = [0u8; 32];
    buf[..by].copy_from_slice(&mem[(mem_len - by)..]);
    mem.copy_within(..(mem_len - by), by);
    mem[..by].copy_from_slice(&buf[..by]);
}

fn rotate_by_reverse(mem: &mut [u8], by: usize) {
    if mem.len() <= by && by >= 1 {
        return
    }

    for _ in 0..(by / 32) {
        rotate_by_reverse_fixed(mem, 32);
    }

    if by % 32 > 0 {
        rotate_by_reverse_fixed(mem, by % 32)
    }
}

fn rotate_by_reverse_fixed(mem: &mut [u8], by: usize) {
    assert!(by <= 32);
    assert!(mem.len() >= by);
    let mem_len = mem.len();
    let mut buf = [0u8; 32];
    buf[..by].copy_from_slice(&mem[..by]);
    mem.copy_within(by.., 0);
    mem[(mem_len - by)..].copy_from_slice(&buf[..by]);
}

fn can_first_be_put_onto_second(selected: u8, open_card: u8) -> bool {
    debug_assert!(selected >> UNO_CARD_KIND_OFF != UNO_CARD_CHOOSE_COLOR_COLORED);
    debug_assert!(open_card >> UNO_CARD_KIND_OFF != UNO_CARD_CHOOSE_COLOR_BLACK);

    let selected_kind = selected >> UNO_CARD_KIND_OFF;
    let open_kind = open_card >> UNO_CARD_KIND_OFF;
    let same_color = selected & UNO_CARD_COLOR_MASK == open_card & UNO_CARD_COLOR_MASK;
    let same_kind = selected_kind == open_kind;
     same_color || same_kind || selected_kind == UNO_CARD_CHOOSE_COLOR_BLACK || selected_kind == UNO_CARD_DRAW_FOUR || open_kind == UNO_CARD_DRAW_FOUR
}

fn post_process_open_card(card: u8) -> u8 {
    let card_is_color_choose = (card >> UNO_CARD_KIND_OFF) == UNO_CARD_CHOOSE_COLOR_COLORED;
    let processed_card = card ^ (card_is_color_choose as u8 * (UNO_CARD_CHOOSE_COLOR_COLORED ^ UNO_CARD_CHOOSE_COLOR_BLACK));
    debug_assert_ne!(processed_card >> UNO_CARD_KIND_OFF, UNO_CARD_CHOOSE_COLOR_COLORED);
    processed_card
}

fn is_draw_card(card: u8) -> bool {
    let card_kind = card >> UNO_CARD_KIND_OFF;
    (card_kind == UNO_CARD_DRAW_FOUR) | (card_kind == UNO_CARD_DRAW_TWO)
}

fn get_signed_direction(direction: u64)-> i64 {
    debug_assert!(direction == 0 ||direction == 1);
    -1 + 2 * direction as i64
}

fn next_player(max_player_value: u64, current_player: u64, next_player_direction: i64) -> u64 {
    let max_player_value = max_player_value + 2;

    let current_player = current_player + max_player_value * ((current_player == 0 || next_player_direction < 0) as u64);
    let mut next_player = current_player.wrapping_add_signed(next_player_direction);
    next_player = next_player - (next_player >= max_player_value) as u64 * max_player_value;
    next_player
}

fn generate_random_num(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

fn card_num_to_card_repr(card: u8) -> CardRepr {
    let card_kind = card >> UNO_CARD_KIND_OFF;
    if card_kind == UNO_CARD_CHOOSE_COLOR_BLACK {
        return CardRepr::Special(SpecialCardKind::ChooseColor)
    } else if card_kind == UNO_CARD_DRAW_FOUR {
        return CardRepr::Special(SpecialCardKind::DrawFour)
    }
    let card_color = card & 0b11;
    let card_color = match card_color {//Red = 0, Blue = 1, Green = 2, Yellow = 3
        0 => CardColor::Red,
        1 => CardColor::Blue,
        2 => CardColor::Green,
        3 => CardColor::Yellow,
        _ => unreachable!()
    };
    let card_kind = if card_kind < 10 {
        let number_card_kind = match card_kind {
            0 => NumberCardKind::Zero,
            1 => NumberCardKind::One,
            2 => NumberCardKind::Two,
            3 => NumberCardKind::Three,
            4 => NumberCardKind::Four,
            5 => NumberCardKind::Five,
            6 => NumberCardKind::Six,
            7 => NumberCardKind::Seven,
            8 => NumberCardKind::Eight,
            9 => NumberCardKind::Nine,
            _ => unreachable!()
        };
        ColoredCardKind::Number(number_card_kind)
    } else {
        let effect_kind = match card_kind {
            UNO_CARD_SKIP => EffectCardKind::Skip,
            UNO_CARD_DRAW_TWO => EffectCardKind::DrawTwo,
            UNO_CARD_REVERSE => EffectCardKind::Reverse,
            UNO_CARD_CHOOSE_COLOR_COLORED => EffectCardKind::ChosenColor,
            _ => unreachable!()
        };
        ColoredCardKind::Effect(effect_kind)
    };
    CardRepr::Colored(card_color, card_kind)
}

fn card_repr_to_card_num(card_repr: CardRepr) -> u8 {
    match card_repr {
        CardRepr::Colored(card_color, card_kind) => {
            let card_color = card_color as u8;
            let card_kind = match card_kind {
                ColoredCardKind::Number(n) => n as u8,
                ColoredCardKind::Effect(EffectCardKind::ChosenColor) => UNO_CARD_CHOOSE_COLOR_COLORED,
                ColoredCardKind::Effect(EffectCardKind::Reverse) => UNO_CARD_REVERSE,
                ColoredCardKind::Effect(EffectCardKind::DrawTwo) => UNO_CARD_DRAW_TWO,
                ColoredCardKind::Effect(EffectCardKind::Skip) => UNO_CARD_SKIP
            };
            (card_kind << UNO_CARD_KIND_OFF) | card_color
        }
        CardRepr::Special(SpecialCardKind::ChooseColor) => UNO_CARD_CHOOSE_COLOR_BLACK << UNO_CARD_KIND_OFF,
        CardRepr::Special(SpecialCardKind::DrawFour) => UNO_CARD_DRAW_FOUR << UNO_CARD_KIND_OFF
    }
}

#[cfg(test)]
mod tests {
    use crate::monte_carlo_game::GameWithMoves;
    use crate::uno_basic_game::{can_first_be_put_onto_second, card_num_to_card_repr, card_repr_to_card_num, CardColor, CardRepr, ColoredCardKind, EffectCardKind, NumberCardKind, PlayerAmount, rotate_by, rotate_by_reverse, SpecialCardKind, Uno, UNO_CARD_CHOOSE_COLOR_BLACK, UNO_CARD_CHOOSE_COLOR_COLORED, UNO_CARD_KIND_OFF, UnoMoveEnum};

    impl From<(CardColor, NumberCardKind)> for CardRepr {
        fn from((color, kind): (CardColor, NumberCardKind)) -> Self {
            CardRepr::Colored(color, ColoredCardKind::Number(kind))
        }
    }

    impl From<(CardColor, EffectCardKind)> for CardRepr {
        fn from((color, kind): (CardColor, EffectCardKind)) -> Self {
            CardRepr::Colored(color, ColoredCardKind::Effect(kind))
        }
    }

    impl From<SpecialCardKind> for CardRepr {
        fn from(value: SpecialCardKind) -> Self {
            CardRepr::Special(value)
        }
    }

    fn card_num<R>(r: R) -> u8 where CardRepr: From<R> {
        card_repr_to_card_num(CardRepr::from(r))
    }

    #[test]
    fn test_can_be_put_on() {
        let card_blue_7 = card_num((CardColor::Blue, NumberCardKind::Seven));
        let card_blue_2 = card_num((CardColor::Blue, NumberCardKind::Two));
        let card_blue_chosen = card_num((CardColor::Blue, EffectCardKind::ChosenColor));
        let card_red_7 = card_num((CardColor::Red, NumberCardKind::Seven));
        let card_red_skip = card_num((CardColor::Red, EffectCardKind::Skip));
        let card_red_chosen = card_num((CardColor::Red, EffectCardKind::ChosenColor));
        let card_draw_four = card_num(SpecialCardKind::DrawFour);
        let card_choose_color_black = card_num(SpecialCardKind::ChooseColor);

        let cards = [card_blue_7, card_blue_2, card_blue_chosen, card_red_7, card_red_skip, card_red_chosen, card_draw_four, card_choose_color_black];
        let mut eq_sym = vec![(card_blue_7, card_blue_2), (card_blue_2, card_blue_chosen), (card_blue_7, card_red_7), (card_blue_7, card_blue_chosen), (card_red_7, card_red_skip), (card_red_skip, card_red_chosen), (card_red_7, card_red_chosen)];
        eq_sym.extend(cards.iter().map(|card| (*card, card_draw_four)));

        let mut eq_unsym = cards.iter().map(|card|(card_choose_color_black, *card)).collect::<Vec<_>>();

        let mut can_be_put_on = eq_unsym;
        can_be_put_on.extend(eq_sym.into_iter().flat_map(|(c1, c2)| [(c1, c2), (c2, c1)]));
        can_be_put_on.extend(cards.map(|card| (card, card)));

        let all_cards = cards.iter().copied().flat_map(|card| cards.iter().copied().map(move |card2| (card, card2)));

        for (card1, card2) in all_cards {
            if card1 >> UNO_CARD_KIND_OFF == UNO_CARD_CHOOSE_COLOR_COLORED {
                continue;
            }
            if card2 >> UNO_CARD_KIND_OFF == UNO_CARD_CHOOSE_COLOR_BLACK {
                continue;
            }

            let expected = can_be_put_on.contains(&(card1, card2));
            let actual = can_first_be_put_onto_second(card1, card2);
            assert_eq!(expected, actual, "can you put {:?} on {:?}? Expected {}, but was {}", card_num_to_card_repr(card1), card_num_to_card_repr(card2), expected, actual);
        }
    }

    #[test]
    fn test_rotate_by() {
        let mut mem = [1, 2, 3, 4, 5, 6, 7];
        rotate_by(&mut mem, 2);
        assert_eq!(mem, [6, 7, 1, 2, 3, 4, 5]);

        let mut mem = [1, 2, 3, 4, 5, 6, 7, 8];
        rotate_by(&mut mem, 2);
        assert_eq!(mem, [7, 8, 1, 2, 3, 4, 5, 6]);

        let mut mem = (0..200).collect::<Vec<_>>();
        rotate_by(&mut mem, 50);
        assert_eq!(mem, (150..200).chain(0..150).collect::<Vec<u8>>())
    }

    #[test]
    fn test_rotate_by_reverse() {
        let mut mem = [1, 2, 3, 4, 5, 6, 7];
        rotate_by_reverse(&mut mem, 2);
        assert_eq!(mem, [3, 4, 5, 6, 7, 1, 2]);

        let mut mem = [1, 2, 3, 4, 5, 6, 7, 8];
        rotate_by_reverse(&mut mem, 2);
        assert_eq!(mem, [3, 4, 5, 6, 7, 8, 1, 2]);

        let mut mem = (0..200).collect::<Vec<u8>>();
        rotate_by_reverse(&mut mem, 50);
        assert_eq!(mem, (50..200).chain(0..50).collect::<Vec<u8>>())
    }

    #[test]
    fn test_normal_round() {
        let mut uno = Uno::new(442522441, PlayerAmount::Two);
        let mut p1_cards = uno.get_p_cards(0).unwrap().collect::<Vec<_>>();
        let mut p2_cards = uno.get_p_cards(1).unwrap().collect::<Vec<_>>();

        let card_index_1 = {
            let open_card = uno.get_open_card();
            let (c_i, _) = p1_cards.iter().enumerate()
                .find(|(_, card)| can_first_be_put_onto_second(**card, open_card))
                .unwrap();
            c_i
        };
        dbg!(&p1_cards, &p2_cards, p1_cards[card_index_1]);
        uno.execute_move(&UnoMoveEnum::ChooseCard(card_index_1.try_into().unwrap()).into()).unwrap();
        let card_index_2 = {
            let open_card = uno.get_open_card();
            assert_eq!(p1_cards[card_index_1], open_card);
            let (c_i, _) = p2_cards.iter().enumerate()
                .find(|(_, card)| can_first_be_put_onto_second(**card, open_card))
                .unwrap();
            c_i
        };

        uno.execute_move(&UnoMoveEnum::ChooseCard(card_index_2.try_into().unwrap()).into()).unwrap();
        assert_eq!(p2_cards[card_index_2], uno.get_open_card());
        p1_cards.remove(card_index_1);
        p2_cards.remove(card_index_2);
        assert_eq!(uno.get_p_cards(0).unwrap().collect::<Vec<_>>(), p1_cards);
        assert_eq!(uno.get_p_cards(1).unwrap().collect::<Vec<_>>(), p2_cards);
    }
}
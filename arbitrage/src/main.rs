#![allow(dead_code)]
#![allow(unused_imports)]
use rand::Rng;
use std::cell::Cell;
use std::sync::RwLock;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(PartialEq)]
enum Token {
    Eth,
    Dai,
}

struct Pool {
    token_x: Token,
    token_y: Token,
    x: RwLock<f64>,
    y: RwLock<f64>,
    k: RwLock<f64>,
}

struct Trader {
    amt_eth: RwLock<f64>,
    amt_dai: RwLock<f64>,
}

fn add(pool: &Pool, add_to_x: f64, add_to_y: f64) {
    *pool.x.write().unwrap() += add_to_x;
    *pool.y.write().unwrap() += add_to_y;
    *pool.k.write().unwrap() = *pool.x.read().unwrap() + *pool.y.read().unwrap();
}

fn remove(pool: &Pool, rem_from_x: f64, rem_from_y: f64) {
    *pool.x.write().unwrap() -= rem_from_x;
    *pool.y.write().unwrap() -= rem_from_y;
    *pool.k.write().unwrap() = *pool.x.read().unwrap() + *pool.y.read().unwrap();
}

fn get_amount_out(amount_in: f64, pool: &Pool, token_in: Token, fee: f64) -> f64 {
    let amount_in_less_fee = amount_in * (1. - fee);
    let py = *pool.y.read().unwrap();
    let px = *pool.x.read().unwrap();
    if token_in == pool.token_x {
        let price = py / px;
        let amount_out = amount_in_less_fee * price;
        if amount_out <= py {
            remove(pool, 0., amount_out);
            add(pool, amount_in, 0.);
            return amount_out;
        } else {
            return 0.;
        }
    } else {
        let price = px / py;
        let amount_out = amount_in_less_fee * price;
        if amount_out <= px {
            remove(pool, amount_out, 0.);
            add(pool, 0., amount_in);
            return amount_out;
        } else {
            return 0.;
        }
    }
}

fn swap(trader: &mut Trader, pool: &Pool, token_in: Token, amount_in: f64, fee: f64) {
    let amt_eth = *trader.amt_eth.read().unwrap();
    let amt_dai = *trader.amt_dai.read().unwrap();
    if token_in == Token::Eth && amt_eth > amount_in {
        let amt_out = get_amount_out(amount_in, pool, token_in, fee);
        if amt_out > 0. {
            *trader.amt_eth.write().unwrap() = amt_eth - amount_in;
            *trader.amt_dai.write().unwrap() = amt_dai + amt_out;
        }
    } else if token_in == Token::Dai && amt_dai > amount_in {
        let amt_out = get_amount_out(amount_in, pool, token_in, fee);
        if amt_out > 0. {
            *trader.amt_dai.write().unwrap() = amt_dai - amt_out;
            *trader.amt_eth.write().unwrap() = amt_dai + amount_in;
        }
    }
}

fn calc_two_pool_arb_profit(x_in: f64, xr1: f64, xr2: f64, yr1: f64, yr2: f64, fee: f64) -> f64 {
    let s = (fee * xr1 * x_in) / (yr1 + (fee * x_in));
    let n = fee * yr2 * s;
    let d = xr2 + fee * s;
    n / d
}

// fn calc_two_pool_arb_optimal_amount(
//     xr1: f64,
//     xr2: f64,
//     yr1: f64,
//     yr2: f64,
//     fee: f64,
// ) -> (f64, f64) {
//     let n1 = (xr1 * xr2 * yr1 * yr2).sqrt() * fee;
//     let n2 = yr1 * yr2;
//     let d = fee * (xr2 + (fee * xr1));
//     ((n1 - n2) / d, ((-n1) - n2) / d)
// }

fn detect_arb(pool1: &Pool, pool2: &Pool, token_in: Token, fee: f64) -> f64 {
    let is_x_1 = token_in == pool1.token_x;
    let is_x_2 = token_in == pool2.token_x;

    let x1 = *pool1.x.read().unwrap();
    let x2 = *pool2.x.read().unwrap();
    let y1 = *pool1.y.read().unwrap();
    let y2 = *pool2.y.read().unwrap();

    let amt_in = 1.;

    let opt_amt = match (is_x_1, is_x_2) {
        (true, true) => calc_two_pool_arb_profit(amt_in, x1, x2, y1, y2, fee),
        (true, false) => calc_two_pool_arb_profit(amt_in, x1, y2, y1, x2, fee),
        (false, false) => calc_two_pool_arb_profit(amt_in, y1, y2, x1, x2, fee),
        (false, true) => calc_two_pool_arb_profit(amt_in, y1, x2, x1, y2, fee),
    };

    opt_amt
}

fn main() {
    let xx = 1000.;
    let yy = 200.;
    let kk = xx * yy;

    let mut pool = Pool {
        token_x: Token::Eth,
        token_y: Token::Dai,
        x: RwLock::new(xx),
        y: RwLock::new(yy),
        k: RwLock::new(xx + yy),
    };

    let mut trader = Trader {
        amt_eth: RwLock::new(xx),
        amt_dai: RwLock::new(yy),
    };

    add(&mut pool, 4., 4.);

    swap(&mut trader, &mut pool, Token::Eth, 2., 0.03);

    let pool1 = Arc::new(Pool {
        token_x: Token::Eth,
        token_y: Token::Dai,
        x: RwLock::new(4.),
        y: RwLock::new(3500.),
        k: RwLock::new(3504.),
    });

    let pool2 = Arc::new(Pool {
        token_x: Token::Eth,
        token_y: Token::Dai,
        x: RwLock::new(4.),
        y: RwLock::new(4000.),
        k: RwLock::new(4004.),
    });

    let borrowpool1 = Arc::clone(&pool1);
    let borrowpool2 = Arc::clone(&pool2);

    let mut handles = vec![];

    let writer = thread::spawn(move || {
        for _ in 1..10 {
            let mut rng = rand::thread_rng();
            let randomness = rng.gen_range(0..10);

            if randomness > 5 {
                add(&borrowpool1, 1., 1000.);
                add(&borrowpool2, 1., 1200.);
            } else {
                remove(&borrowpool1, 1., 1000.);
                remove(&borrowpool2, 1., 1200.);
            }
            thread::sleep(Duration::from_millis(1000));
        }
    });

    let searcher = thread::spawn(move || {
        for _ in 1..10 {
            let b1 = detect_arb(&Arc::clone(&pool1), &Arc::clone(&pool2), Token::Eth, 0.97);
            let b2 = detect_arb(&Arc::clone(&pool2), &Arc::clone(&pool1), Token::Eth, 0.97);
            println!("{:?}", b1);
            println!("{:?}", b2);
            thread::sleep(Duration::from_millis(2000));
        }
    });

    handles.push(writer);
    handles.push(searcher);
    for i in handles {
        i.join().unwrap();
    }
}

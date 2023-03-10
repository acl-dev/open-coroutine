use genawaiter::sync::gen;
use genawaiter::yield_;

fn main() {
    let odd_numbers_less_than_ten = gen!({
        let mut n = 1;
        while n < 10 {
            yield_!(n); // Suspend a function at any point with a value.
            n += 2;
        }
    });

    // Generators can be used as ordinary iterators.
    for num in odd_numbers_less_than_ten {
        println!("{}", num);
    }
}

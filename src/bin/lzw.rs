extern crate campross;
extern crate getopts;

use getopts::Options;
use std::fs::File;
use std::io::Write;
use std::io::{BufReader, BufWriter};
use std::env;

use campross::lzw::{compress, decompress};

fn comp(input: &str, output: &str, stats: bool) {
    {
        let inf = File::open(input).unwrap();
        let outf = File::create(output).unwrap();

        let mut out = compress(BufReader::new(inf), BufWriter::new(outf)).unwrap();
        out.flush().unwrap();
    }

    if stats {
        let inf = File::open(input).unwrap();
        let outf = File::open(output).unwrap();
        let in_size =inf.metadata().unwrap().len();
        let out_size = outf.metadata().unwrap().len();
        println!("Original size: {}", in_size);
        println!("Compressed size: {}", out_size);
        println!("Ratio: {}", out_size as f32 / in_size as f32);
    }
}

fn decomp(input: &str, output: &str, _stats: bool) {
    let inf = File::open(input).unwrap();
    let outf = File::create(output).unwrap();

    let mut out = decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap();
    out.flush().unwrap();
}

/// Print a usage summary to stdout that describes the command syntax.
fn print_usage(program: &str, opts: &Options) {
    let brief = format!("Usage: {} FILE", program);
    print!("{}", opts.usage(&brief));
}

pub fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optopt("i", "input", "set input file", "FILE");
    opts.optopt("o", "output", "set output file", "FILE");
    opts.optflag("c", "compress", "compression mode");
    opts.optflag("d", "decompress", "decompression mode");
    opts.optflag("s", "stats", "print statistics");
    opts.optflag("h", "help", "print this help");

    match opts.parse(&args[1..]) {
        Ok(matches) => {
            if matches.opt_present("h") {
                print_usage(&program, &opts);
            }
            match (matches.opt_str("i"), matches.opt_str("o")) {
                (Some(input), Some(output)) => {
                    let stats = matches.opt_present("s");
                    match (matches.opt_present("c"), matches.opt_present("d")) {
                        (true, false) => {
                            comp(&input, &output, stats);
                        },
                        (false, true) => {
                            decomp(&input, &output, stats);
                        },
                        _ => {
                            println!("must specify either -c or -d");
                            print_usage(&program, &opts);
                        },
                    }
                },
                _ =>
                    print_usage(&program, &opts),
            }
        },
        Err(e) => {
            println!("Error: {}", e);
            print_usage(&program, &opts);
        },
    }

}

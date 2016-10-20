extern crate campross;
extern crate getopts;

use getopts::Options;
use std::fs::File;
use std::io::Write;
use std::io::{BufReader, BufWriter};
use std::env;

use campross::arith;
use campross::lzw;
use campross::lzmg1;
use campross::huff;

enum Method {
    Arith,
    Lzw,
    Lzmg1,
    Huff,
}

fn do_compress(input: &str, output: &str, method: Method, stats: bool) {
    {
        let inf = File::open(input).unwrap();
        let outf = File::create(output).unwrap();

        let mut out = match method {
            Method::Arith => {
                let enc = arith::Encoder::new();
                enc.compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
            },
            Method::Lzw => {
                lzw::compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
            },
            Method::Lzmg1 => {
                lzmg1::compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
            },
            Method::Huff => {
                huff::compress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
            },
        };
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

fn do_decompress(input: &str, output: &str, method: Method, _stats: bool) {
    let inf = File::open(input).unwrap();
    let outf = File::create(output).unwrap();

    let mut out = match method {
        Method::Arith => {
            let enc = arith::Decoder::new();
            enc.decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
        },
        Method::Lzw => {
            lzw::decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
        },
        Method::Lzmg1 => {
            lzmg1::decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
        },
        Method::Huff => {
            huff::decompress(BufReader::new(inf), BufWriter::new(outf)).unwrap()
        },
    };
    out.flush().unwrap();
}

fn do_inspect(input: &str, method: Method) {
    let inf = File::open(input).unwrap();

    match method {
        Method::Arith => {
            println!("inspect mode not supported for arithmetic encoder");
        },
        Method::Lzw => {
            lzw::inspect(BufReader::new(inf)).unwrap();
        },
        Method::Lzmg1 => {
            println!("inspect mode not supported for LZMG1 encoder");
        },
        Method::Huff => {
            println!("inspect mode not supported for Huffman encoder");
        },
    }
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
    opts.optflag("c", "compress", "compress the input file");
    opts.optflag("d", "decompress", "decompress the input file");
    opts.optflag("x", "examine", "examine a compressed file");
    opts.optopt("m", "method", "select compression method", "arith|lzw|lzmg1");
    opts.optflag("s", "stats", "print statistics");
    opts.optflag("h", "help", "print this help");

    match opts.parse(&args[1..]) {
        Ok(matches) => {
            if matches.opt_present("h") {
                print_usage(&program, &opts);
            }
            let method =
                if let Some(s) = matches.opt_str("m") {
                    match &s[..] {
                        "arith" => Some(Method::Arith),
                        "lzw"   => Some(Method::Lzw),
                        "lzmg1"  => Some(Method::Lzmg1),
                        "huff"  => Some(Method::Huff),
                        _       => None,
                    }
                } else {
                    Some(Method::Arith)
                };
            if matches.opt_present("x") {
                if let Some(m) = method {
                    match matches.opt_str("i") {
                        Some(input) => {
                            do_inspect(&input, m);
                        },
                        None => {
                            print_usage(&program, &opts);
                        }
                    }
                } else {
                    print_usage(&program, &opts);
                }
            } else {
            match (matches.opt_str("i"), matches.opt_str("o")) {
                (Some(input), Some(output)) => {
                    let stats = matches.opt_present("s");
                    match (method, matches.opt_present("c"), matches.opt_present("d")) {
                        (Some(m), true, false) => {
                            do_compress(&input, &output, m, stats);
                        },
                        (Some(m), false, true) => {
                            do_decompress(&input, &output, m, stats);
                        },
                        _ => {
                            print_usage(&program, &opts);
                        },
                    }
                },
                _ =>
                    print_usage(&program, &opts),
            }
            }
        },
        Err(e) => {
            println!("Error: {}", e);
            print_usage(&program, &opts);
        },
    }

}

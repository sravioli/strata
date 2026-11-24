//! `strata-extract`: one file on standard input, one JSON document on standard output (0007).

fn main() {
    let code = strata_extract::run(
        strata_extract::PDFTOTEXT,
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    );
    std::process::exit(code.code());
}

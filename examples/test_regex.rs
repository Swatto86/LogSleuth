use regex::Regex;
fn main() {
    let re = Regex::new(r#"\B\[0\]\B"#).unwrap();
    println!("match middle: {}", re.is_match("found [0] in log"));
    println!("match start: {}", re.is_match("[0] in log"));
    println!("match end: {}", re.is_match("found [0]"));
    println!("match alone: {}", re.is_match("[0]"));
    println!("match adjacent w: {}", re.is_match("founda[0] in log"));
    println!("match adjacent w end: {}", re.is_match("found [0]a in log"));
}

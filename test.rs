fn main() {
    let re = regex::Regex::new(r"(?:\B|^)\[0\](?:\B|$)").unwrap();
    println!("match middle: {:?}", re.find("found [0] in log"));
    println!("match start: {:?}", re.find("[0] in log"));
    println!("match end: {:?}", re.find("found [0]"));
    println!("match alone: {:?}", re.find("[0]"));
    println!("no match: {:?}", re.find("error[0]"));
}

use std::collections::HashMap;

fn main() {
    let client = reqwest::Client::new();

    let json_str = r#"
        {"path": "/115/bk_plain/video/AV", "password": "", "page": 1, "per_page": 0, "refresh": false}
    "#;

//     let param = HashMap::from([
// ("path", "/115/bk_plain/video/AV"), "password": "", "page": 1, "per_page": 0, "refresh": false
//     ])

    let res = client
        .post("http://192.168.0.201:5244/api/fs/list")
        .json(json_str)
        .send()
        .await?;

    println!("Hello, world!");
}

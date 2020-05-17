use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

pub struct Chrome {
    id: i32,
    pub cmd: Option<Child>, //TODO:Add fields
}

impl Chrome {
    pub fn new_with_args(chrome_binary: &str, args: Vec<String>) -> Chrome {
        let mut c = Chrome {
            id: 2,
            cmd: Some(
                Command::new(chrome_binary)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .args(args)
                    .spawn()
                    .expect("Cannot spawn chrome"),
            ),
        };
        let stderr = BufReader::new(c.cmd.as_mut().unwrap().stderr.take().unwrap());
        let re = regex::Regex::new(r"^DevTools listening on (ws://.*?)$").unwrap();

        let mut ws_url = String::new();
        for line in stderr.lines() {
            let line=match line {
                Ok(l)=>l,
                Err(e)=>{
                    c.cmd.unwrap().kill().expect("Chrome command is not running");
                    panic!(e)
                }
            };
            match re.captures(&line) {
                None => continue,
                Some(cap) => {
                    ws_url = cap[1].to_string();
                    break;
                }
            }
        }
        //TODO:Initialize Session
        c
    }
}

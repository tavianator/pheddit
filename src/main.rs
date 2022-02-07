use pulldown_cmark::{Parser, html};

use rayon::prelude::*;

use regex::{self, Regex};

use rocket::{State, get, launch, routes};
use rocket::response::content::{Css, Html};

use serde_json::{Value, from_str};

use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};

#[derive(Default)]
struct Posts {
    map: HashMap<String, Value>,
}

#[get("/")]
fn index() -> Html<&'static str> {
    Html(r#"<!DOCTYPE HTML>
        <html>
            <head>
                <title>Pheddit</title>
                <link rel="stylesheet" type="text/css" href="/style.css">
            </head>
            <body>
                <h1>Pheddit search engine</h1>
                <form action="/search" method="get">
                    <label for="query">Query: </label>
                    <input type="search" name="query" id="query" required>
                    <input type="submit" value="Search">
                </form>
            </body>
        </html>
    "#)
}

#[get("/style.css")]
fn style() -> Css<&'static str> {
    Css(r#"
        html {
            height: 100%;
            background: lightgray;
            overflow-y: scroll;
        }

        body {
            display: flow-root;
            min-height: 100%;
            max-width: 800px;
            margin: 0 auto;
            padding: 0 1em;
            background: white;
            box-shadow: 5px 0 5px gray, -5px 0 5px gray;
        }
    "#)
}

fn get_str<'a, 'b>(value: &'a Value, key: &'b str) -> &'a str {
    value.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
}

#[get("/search?<query>")]
fn search(posts: &State<Posts>, query: &str) -> Html<String> {
    let mut res = vec![];
    for word in query.split_whitespace() {
        if let Ok(re) = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(word))) {
            res.push(re);
        }
    }

    let matches: Vec<_> = posts.map.par_iter()
        .map(|(_id, post)| (post, get_str(post, "title"), get_str(post, "selftext")))
        .filter(|(_post, title, text)| res.iter().all(|re| re.is_match(title) || re.is_match(text)))
        .map(|(post, _title, _text)| post)
        .collect();

    let mut output = format!(r#"<!DOCTYPE HTML>
        <html>
            <head>
                <title>Pheddit Search | {query}</title>
                <link rel="stylesheet" type="text/css" href="/style.css">
            </head>
            <body style="max-width: 800px; margin: auto;">
                <h2>{count} results for <em>{query}</em></h2>
                <ul>
    "#, query=query, count=matches.len());

    for post in matches {
        let id = get_str(post, "id");
        let title = get_str(post, "title");
        output += &format!(r#"<li><a href="/post/{id}">{title}</a>"#, id=id, title=title);
    }

    output += r#"
                </ul>
            </body>
        </html>
    "#;

    Html(output)
}

#[get("/post/<id>")]
fn post(posts: &State<Posts>, id: &str) -> Option<Html<String>> {
    let post = posts.map.get(id)?;
    let title = post.get("title")?.as_str()?;
    let text = post.get("selftext")?.as_str()?;

    let mut output = format!(r#"<!DOCTYPE HTML>
        <html>
            <head>
                <title>Pheddit | {title}</title>
                <link rel="stylesheet" type="text/css" href="/style.css">
            </head>
            <body style="max-width: 800px; margin: auto;">
                <h1>{title}</h1>
    "#, title=title);
    html::push_html(&mut output, Parser::new(text));
    output += "
            </body>
        </html>
    ";

    Some(Html(output))
}

#[get("/candidates/<n>")]
fn candidates(posts: &State<Posts>, n: usize) -> Html<String> {
    let queries = vec![
        "degree",
        "career", "careers",
        "programming",
        "school",
        "learn", "learning",
        "switch", "switching",
        "change", "changing",
        "college", "university",
        "advice",
        "bootcamp", "bootcamps", "camp", "camps",
        "self taught",
    ];

    let mut res = vec![];
    for query in &queries {
        let mut terms = vec![];
        for word in query.split_whitespace() {
            if let Ok(re) = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(word))) {
                terms.push(re);
            }
        }
        res.push(terms);
    }

    let mut matches: Vec<_> = posts.map.par_iter()
        .map(|(_id, post)| (post, get_str(post, "title"), get_str(post, "selftext")))
        .filter(|(_post, title, text)| {
            res.iter().any(|terms| {
                terms.iter().all(|re| re.is_match(title) || re.is_match(text))
            })
        })
        .map(|(post, _title, _text)| post)
        .collect();

    matches.sort_by_key(|post| get_str(post, "id"));

    let start = n * matches.len() / 3;
    let end = (n + 1) * matches.len() / 3;

    let mut output = format!(r#"<!DOCTYPE HTML>
        <html>
            <head>
                <title>Pheddit Candidates | {n}/3</title>
                <link rel="stylesheet" type="text/css" href="/style.css">
            </head>
            <body style="max-width: 800px; margin: auto;">
                <h2>Candidates {start}–{end} of {count}</em></h2>
                <ul>
    "#, n=n, start=start, end=end, count=matches.len());

    for post in &matches[start..end] {
        let id = get_str(post, "id");
        let title = get_str(post, "title");
        output += &format!(r#"<li><a href="/post/{id}">{title}</a>"#, id=id, title=title);
    }

    output += r#"
                </ul>
            </body>
        </html>
    "#;

    Html(output)
}

#[launch]
fn rocket() -> _ {
    let paths: Vec<_> = env::args()
        .skip(1)
        .flat_map(|arg| fs::read_dir(&arg).unwrap())
        .map(|file| file.unwrap().path())
        .filter(|path| path.extension().map_or(false, |e| e == "json"))
        .collect();

    let posts = Posts {
        map: paths.par_iter()
            .map(|path| File::open(path).unwrap())
            .map(BufReader::new)
            .flat_map_iter(|reader| reader.lines())
            .map(Result::unwrap)
            .map(|line| from_str::<Value>(&line).unwrap())
            .map(|post| (get_str(&post, "id").to_string(), post))
            .collect(),
    };
    eprintln!("Loaded {} posts...", posts.map.len());

    rocket::build()
        .manage(posts)
        .mount("/", routes![index, style, search, post, candidates])
}

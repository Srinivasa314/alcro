use alcro::{Content, UIBuilder};

#[tokio::test(flavor = "multi_thread")]
async fn test_content() {
    let ui = UIBuilder::new()
        .content(Content::Html("<html><body>Close Me!</body></html>"))
        .custom_args(&["--headless"])
        .run()
        .await
        .expect("Unable to launch");
    assert_eq!(
        ui.eval("document.body.innerText").await.unwrap(),
        "Close Me!"
    );

    let ui2 = UIBuilder::new()
        .content(Content::Url("https://www.google.com"))
        .custom_args(&["--headless"])
        .run()
        .await
        .expect("Unable to launch");
    assert_eq!(
        ui2.eval("window.location.href").await.unwrap(),
        "https://www.google.com/"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_eval() {
    let ui = UIBuilder::new()
        .custom_args(&["--headless"])
        .run()
        .await
        .expect("Unable to launch");
    assert_eq!(ui.eval("2+2").await.unwrap(), 4);
    assert_eq!(
        ui.eval("Promise.resolve('Its Ok')").await.unwrap(),
        "Its Ok"
    );
    assert_eq!(ui.eval("Promise.reject('ERROR')").await.unwrap_err(), "ERROR");
    assert_eq!(ui.eval("throw 'ERROR'").await.unwrap_err(), "ERROR");
    assert!(ui.eval("dtyfhgxnt*").await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_bind() {
    let ui = UIBuilder::new()
        .content(Content::Html(
            r#"
        <script>
        async function foo(x) {
          const result = await bar(x + 'b');
          return result + 'd';
        }
        </script>
        "#,
        ))
        .custom_args(&["--headless"])
        .run()
        .await
        .expect("Unable to launch");

    ui.bind("bar", |args| async move {
        tokio::task::yield_now().await;
        Ok(format!("{}c", args[0].as_str().expect("arg to be str")).into())
    })
    .await
    .unwrap();

    assert_eq!(ui.eval("foo('a')").await.unwrap(), "abcd");
}

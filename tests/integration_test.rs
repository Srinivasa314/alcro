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
        .content(Content::Url("https://example.com"))
        .custom_args(&["--headless"])
        .run()
        .await
        .expect("Unable to launch");
    assert_eq!(
        ui2.eval("window.location.href").await.unwrap(),
        "https://example.com/"
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
async fn test_multiple_windows() {
    let ui = UIBuilder::new()
        .content(Content::Html("<html><body>first</body></html>"))
        .custom_args(&["--headless"])
        .run()
        .await
        .expect("Unable to launch");
    let ui2 = ui
        .new_window(Content::Html("<html><body>second</body></html>"))
        .await
        .expect("Unable to open second window");

    assert_eq!(ui.eval("document.body.innerText").await.unwrap(), "first");
    assert_eq!(ui2.eval("document.body.innerText").await.unwrap(), "second");

    // Bindings are per window
    ui2.bind("who", |_| async move { Ok("second".into()) })
        .await
        .unwrap();
    assert_eq!(ui2.eval("(async () => await who())()").await.unwrap(), "second");
    assert_eq!(ui.eval("typeof who").await.unwrap(), "undefined");

    // Closing one window leaves the other usable
    ui2.close().await;
    ui2.wait_finish().await;
    assert!(ui2.done());
    assert!(!ui.done());
    assert_eq!(ui.eval("1+1").await.unwrap(), 2);
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

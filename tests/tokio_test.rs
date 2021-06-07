use alcro::{Content, UIBuilder};

#[tokio::test(flavor = "multi_thread")]
async fn test_bind_tokio() {
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
        .expect("Unable to launch");

    ui.bind_tokio("bar", move |args| async move {
        tokio::task::yield_now().await;
        Ok(format!("{}c", args[0].as_str().expect("arg to be str")).into())
    })
    .unwrap();

    assert_eq!(ui.eval("foo('a')").unwrap(), "abcd");
}

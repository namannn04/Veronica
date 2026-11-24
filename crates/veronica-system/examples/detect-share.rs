fn main() -> anyhow::Result<()> {
    let state = tokio::runtime::Runtime::new()?.block_on(veronica_system::screencast::detect());
    println!("{}", serde_json::to_string_pretty(&state)?);
    Ok(())
}

use subject as vthread;

fn main() -> vthread::Result<()> {
    vthread::run(|scope| {
        let mut answer = scope.spawn("answer", || 52)?;
        assert_eq!(answer.join()?, 52);
        Ok(())
    })
}

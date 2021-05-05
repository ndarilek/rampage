local Pipeline(platform) = {
  kind: "pipeline",
  name: "build",
  steps: [
    {
      name: "test",
      image: "rust",
      commands: [
        "cargo check --verbose --all",
      ]
    }
  ]
};

[
  Pipeline("linux"),
]
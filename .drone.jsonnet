local Pipeline(platform) = {
  kind: "pipeline",
  name: "build",
  steps: [
    {
      name: "test",
      image: "rust",
      commands: [
        "apt-get update -qq",
        "apt-get install -qqy libspeechd-dev pkg-config libx11-dev libasound2-dev libudev-dev zip",
        "cargo check --verbose --all",
      ]
    },
    {
      name: "build release",
      image: "rust",
      commands: [
        "cargo install cargo-make",
        "cargo make -p release build"
      ],
      when: {branch: ["release"]}
    }
  ]
};

[
  Pipeline("linux"),
]
local Pipeline(platform) = {
  kind: "pipeline",
  name: "build",
  clone: {
    disable: true
  },
  steps: [
    {
      name: "clone",
      image: "alpine/git",
      commands: [
        "apk add --no-cache git-lfs",
        "git clone --recursive $DRONE_REPO_LINK .",
        "git checkout $DRONE_COMMIT"
      ]
    },
    {
      name: "test",
      image: "rust",
      commands: [
        "apt-get update -qq",
        "apt-get install -qqy libclang-dev libspeechd-dev pkg-config libx11-dev libasound2-dev libudev-dev zip",
        "cargo check --all",
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
module.exports = {
  apps: [
    {
      name: "axelar-relayer",
      script: "mock-axelar-relayer/index.js",
      watch: false,
    },
    {
      name: "payload-cache",
      script: "mock-payload-cache/index.js",
      watch: false,
    },
    {
      name: "gmp-api",
      script: "mock-gmp-api/index.js",
      watch: false,
    }
  ]
};


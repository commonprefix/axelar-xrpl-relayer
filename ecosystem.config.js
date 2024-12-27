module.exports = {
  apps: [
    {
      name: "axelar-relayer",
      script: "mock-axelar-relayer/index.js",
      watch: true,
    },
    {
      name: "payload-cache",
      script: "mock-payload-cache/index.js",
      watch: true,
    }
    {
      name: "gmp-api",
      script: "mock-gmp-api/index.js",
      watch: true,
    }
  ]
};


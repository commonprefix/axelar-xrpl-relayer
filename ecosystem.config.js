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
      name: "gmp-api-devnet-its",
      script: "mock-gmp-api/index.js",
      watch: false,
      env: {
        ENV_PATH: ".env_devnet-its",
        PORT: "3001",
      }
    },
    {
      name: "gmp-api-devnet-amplifier",
      script: "mock-gmp-api/index.js",
      watch: false,
      env: {
        ENV_PATH: ".env_devnet-amplifier",
        PORT: "3002",
      }
    }
  ]
};


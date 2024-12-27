require('dotenv').config({ path: __dirname + '/../.env' });
const Sentry = require("@sentry/node");

Sentry.init({
    dsn: process.env.AXELAR_RELAYER_SENTRY_DSN,
    integrations: [],
    // Tracing
    tracesSampleRate: 1.0, //  Capture 100% of the transactions
});
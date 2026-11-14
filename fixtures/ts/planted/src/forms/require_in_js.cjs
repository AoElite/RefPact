const client = require("../db/client"); //~ require src/db/client.ts

module.exports = { rows: client.query("x") }; //~ require src/db/client.ts#query

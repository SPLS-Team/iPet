// iPet local-tool stdio demo.
//
// Contract: read one JSON line from stdin (the model's arguments), write the
// result to stdout. iPet closes stdin after writing, so reading until EOF is
// correct. Non-zero exit or stderr output is surfaced as an error by iPet.
let input = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  input += chunk;
});
process.stdin.on("end", () => {
  let args = {};
  try {
    args = input.trim() ? JSON.parse(input) : {};
  } catch (err) {
    process.stderr.write(`参数不是合法 JSON: ${err}\n`);
    process.exit(1);
  }
  const text = typeof args.text === "string" ? args.text : "";
  // Echo back as JSON — the model will see this verbatim.
  process.stdout.write(JSON.stringify({ echo: text, length: text.length }));
});

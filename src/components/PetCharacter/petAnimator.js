export function setPetMood(root, mood) {
  root.dataset.mood = mood;
}

export function randomLocalReply() {
  const replies = [
    "我在。",
    "状态正常。",
    "收到。",
    "可以继续。",
    "先看一下系统情况。",
  ];
  return replies[Math.floor(Math.random() * replies.length)];
}


import { randomLocalReply, setPetMood } from "./petAnimator.js";

export function createPetCharacter(root) {
  root.className = "pet-stage";
  root.innerHTML = `
    <div class="pet-shadow"></div>
    <button class="pet-body" type="button" aria-label="pet">
      <span class="pet-ear pet-ear-left"></span>
      <span class="pet-ear pet-ear-right"></span>
      <span class="pet-face">
        <span class="pet-eye pet-eye-left"></span>
        <span class="pet-eye pet-eye-right"></span>
        <span class="pet-mouth"></span>
      </span>
      <span class="pet-core"></span>
    </button>
    <div class="pet-status-line" data-role="line">iPet</div>
  `;

  const body = root.querySelector(".pet-body");
  const line = root.querySelector('[data-role="line"]');

  body.addEventListener("click", () => {
    setPetMood(root, "talking");
    line.textContent = randomLocalReply();
    window.setTimeout(() => setPetMood(root, "idle"), 900);
  });

  setPetMood(root, "idle");

  return {
    setMood(mood) {
      setPetMood(root, mood);
    },
    setLine(text) {
      line.textContent = text || "iPet";
    },
  };
}

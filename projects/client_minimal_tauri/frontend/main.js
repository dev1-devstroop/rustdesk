import init, { connect_to_server, set_mode, send_mouse_move, send_mouse_click, send_key_press } from '../src-tauri/tauri.js';

(async () => {
  await init();

  const status = document.getElementById('status');
  const canvas = document.getElementById('canvas');
  const ctx = canvas.getContext('2d');

  let width = 0, height = 0;

  document.getElementById('connect').onclick = async () => {
    const host = document.getElementById('host').value;
    const port = parseInt(document.getElementById('port').value, 10);
    status.textContent = 'Connecting...';

    try {
      await connect_to_server(host, port);
      status.textContent = 'Connected';
      await set_mode('desktop', 0);
    } catch (e) {
      status.textContent = 'Error: ' + e;
    }
  };

  window.addEventListener('frame', event => {
    const { width: w, height: h, data } = event.detail;
    if (w !== width || h !== height) {
      width = w;
      height = h;
      canvas.width = width;
      canvas.height = height;
    }
    const imgData = new ImageData(new Uint8ClampedArray(data), width, height);
    ctx.putImageData(imgData, 0, 0);
  });

  canvas.onmousemove = e => send_mouse_move(e.offsetX, e.offsetY);
  canvas.onmousedown = e => send_mouse_click(e.button, true);
  canvas.onmouseup = e => send_mouse_click(e.button, false);
  window.onkeydown = e => send_key_press(e.key, true);
  window.onkeyup = e => send_key_press(e.key, false);
})();

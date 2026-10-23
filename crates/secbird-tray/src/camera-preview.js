// Browser-owned capture: no frame copies, recording, microphone, or server stream.
(function (root) {
  'use strict';
  const presets = [[1920, 1080, 60], [1920, 1080, 30], [1280, 720, 60], [1280, 720, 30], [640, 480, 30], [640, 480, 15]];
  function exactConstraints(deviceId, format) {
    if (typeof deviceId !== 'string' || !deviceId) throw new Error('Select a preview camera first.');
    const video = {deviceId: {exact: deviceId}};
    if (format) {
      if (!Number.isInteger(format.width) || format.width <= 0 || !Number.isInteger(format.height) || format.height <= 0 || !Number.isFinite(format.frameRate) || format.frameRate <= 0) {
        throw new Error('Preview requires positive whole-pixel dimensions and a finite positive FPS.');
      }
      for (const key of ['width', 'height', 'frameRate']) video[key] = {exact: format[key]};
    }
    return {audio: false, video};
  }
  function candidateFormats(capabilities = {}) {
    return presets.map(([width, height, frameRate]) => ({width, height, frameRate})).filter(format =>
      Object.entries(format).every(([key, value]) => {
        const range = capabilities[key];
        return !range || (value >= range.min && value <= range.max);
      }));
  }
  const release = stream => stream?.getTracks().forEach(track => track.stop());
  const actualOf = track => {
    const {width, height, frameRate} = track.getSettings();
    return {width, height, frameRate};
  };
  const matches = (wanted, actual) => !wanted || (wanted.width === actual.width && wanted.height === actual.height && Math.abs(wanted.frameRate - actual.frameRate) < 0.1);
  const explain = error => ({
    NotAllowedError: 'Camera permission denied. Allow camera access in your browser and Windows privacy settings.',
    NotReadableError: 'Camera is busy or inaccessible. Stop preview in OBS, Teams, or other camera apps and try again.',
    NotFoundError: 'Selected camera is unavailable. Refresh sources and select it again.',
  }[error.name] || error.message || 'Camera preview failed.');

  function createPreview({mediaDevices, video, onState = () => {}}) {
    let generation = 0, stream = null, state = 'stopped', actual = null, error = null, sourceId = null, formatBusy = false;
    const snapshot = () => ({state, actual: actual && {...actual}, error});
    const publish = () => onState(snapshot());
    const requireMedia = () => {
      if (!mediaDevices?.getUserMedia || !mediaDevices?.enumerateDevices) throw new Error('This browser does not provide camera access. Open the local editor in a current browser.');
    };
    function stop() {
      generation++;
      const previous = stream;
      stream = null; sourceId = null; actual = null; state = 'stopped'; error = null; formatBusy = false;
      video.srcObject = null;
      release(previous);
      publish();
    }
    function failed(cause) {
      stop(); state = 'error'; error = explain(cause); publish();
    }
    async function sources(requestPermission = false) {
      requireMedia();
      if (requestPermission) {
        const temporary = await mediaDevices.getUserMedia({audio: false, video: true});
        release(temporary);
      }
      return (await mediaDevices.enumerateDevices()).filter(device => device.kind === 'videoinput')
        .map(({deviceId, label}) => ({deviceId, label}));
    }
    async function start(deviceId, format = null) {
      stop();
      const epoch = generation;
      let acquired = null;
      try {
        requireMedia();
        const constraints = exactConstraints(deviceId, format);
        state = 'starting'; publish();
        acquired = await mediaDevices.getUserMedia(constraints);
        if (epoch !== generation) { release(acquired); return; }
        const track = acquired.getVideoTracks()[0];
        if (!track || acquired.getAudioTracks().length || track.getSettings().deviceId !== deviceId) throw new Error('The browser returned a different preview source. Select the intended camera explicitly.');
        const negotiated = actualOf(track);
        if (!matches(format, negotiated)) throw new Error('The requested preview format was not negotiated. Try a different combination.');
        stream = acquired; sourceId = deviceId; actual = negotiated;
        track.addEventListener('ended', () => {
          if (stream === acquired) failed(new Error('Camera preview ended or disconnected. Restart it explicitly when ready.'));
        });
        video.srcObject = stream;
        await video.play();
        if (epoch !== generation) { release(acquired); return; }
        state = 'playing'; publish();
      } catch (cause) {
        release(acquired);
        if (epoch !== generation) return;
        failed(cause); throw cause;
      }
    }
    async function setFormat(format) {
      if (!stream || state !== 'playing') throw new Error('Start preview before changing its format.');
      if (formatBusy) throw new Error('A preview format change is already in progress.');
      const wanted = exactConstraints(sourceId, format).video;
      const epoch = generation, track = stream.getVideoTracks()[0], previous = actualOf(track);
      const restore = exactConstraints(sourceId, previous).video;
      formatBusy = true;
      try {
        await track.applyConstraints(wanted);
        if (epoch !== generation) return;
        const negotiated = actualOf(track);
        if (!matches(format, negotiated)) throw new Error('Requested preview format was not negotiated.');
        actual = negotiated; error = null; publish();
      } catch (cause) {
        if (epoch !== generation) return;
        try {
          await track.applyConstraints(restore);
          if (epoch !== generation) return;
          if (!matches(previous, actualOf(track))) throw new Error('Previous preview format could not be restored.');
          actual = actualOf(track); error = explain(cause); publish();
        } catch (restoreError) {
          if (epoch === generation) failed(restoreError);
        }
        throw cause;
      } finally { if (epoch === generation) formatBusy = false; }
    }
    function refresh() {
      if (!stream) return;
      actual = actualOf(stream.getVideoTracks()[0]); publish();
    }
    const formats = () => candidateFormats(stream?.getVideoTracks()[0]?.getCapabilities?.() || {});
    return {sources, start, setFormat, stop, snapshot, refresh, formats};
  }
  const api = {createPreview, exactConstraints, candidateFormats};
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.SecbirdPreview = api;
})(globalThis);

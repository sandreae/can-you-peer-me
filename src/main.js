const { invoke, Channel } = window.__TAURI__.core;

const peers = new Object();

async function init() {
  // Load all samples
  samplePaths.forEach(async (path, index) => {
    console.log("load sample", path);
    await loadSample(path, index);
  });

  // Create the stream channel to be passed to backend and add an `onMessage`
  // callback method to handle any events which are later sent from the
  // backend to here.
  const channel = new Channel();
  channel.onmessage = processor;
  // The start command must be called on app startup otherwise running the node
  // on the backend is blocked. This is because we need the stream channel to
  // be provided and passed into the node stream receiver task.
  await invoke("init", { channel });
}

window.addEventListener("DOMContentLoaded", () => {
  console.log("init");
  init();
});

// Create an audio context
const audioContext = new (window.AudioContext || window.webkitAudioContext)();

// List of sample filenames
const samplePaths = [
  "./assets/528683__kjose__metalpot_strongfingertap2.flac",
  "./assets/528682__kjose__metalslap_rim.flac",
  "./assets/528681__kjose__metalpot_strongfingertap1.flac",
  "./assets/528680__kjose__metalpot_mutedfingertap2.flac",
  "./assets/528679__kjose__metalpot_sideslap.flac",
  "./assets/528678__kjose__metalpot_midslap.wav",
  "./assets/528677__kjose__metalpot_mutedfingertap1.flac",
  "./assets/528676__kjose__metalpot_edgeslap_muff.flac",
  "./assets/528675__kjose__metalpot_edgeslap_mute.flac",
  "./assets/528674__kjose__metalpot_fingertap1.flac",
  "./assets/528673__kjose__metalpot_fingertap2.flac",
  "./assets/528672__kjose__metalpot-sideslap_mute.flac",
  "./assets/528671__kjose__metalpot_edgeslap-soft.flac",
  "./assets/528670__kjose__metalpot_edgeslap-strong.flac",
  "./assets/528669__kjose__metalpot_edgeslap.flac",
  "./assets/771248__olehenriksen__kitchen-faucet-water-tap-running-water-sound.wav",
  "./assets/609725__theplax__microwave-ping.wav",
  "./assets/431117__inspectorj__door-front-opening-a.wav",
  "./assets/90030__tewell__oneascendingquack.wav",
  "./assets/442820__qubodup__duck-quack.wav",
  "./assets/196124__enma-darei__dropped-stuff.wav",
];

// Array to hold the audio buffers
const audioBuffers = [];

// Function to load an audio sample
async function loadSample(url, index) {
  fetch(url)
    .then((response) => response.arrayBuffer())
    .then((arrayBuffer) => audioContext.decodeAudioData(arrayBuffer))
    .then((audioBuffer) => {
      audioBuffers[index] = audioBuffer;
    })
    .catch((error) => console.error("Error loading sample:", error));
}

// Function to play a sample
function playSample(index) {
  const source = audioContext.createBufferSource();
  source.buffer = audioBuffers[index];
  source.connect(audioContext.destination);
  source.start(audioContext.currentTime, 0, 5);
  return source;
}

function broadcastSample(index) {
  let timestamp = Date.now();

  console.log("broadcast: ", timestamp, index);
  invoke("broadcast", { timestamp, index });
}

function processor(message) {
  switch (message.type) {
    case "SamplePlayed": {
      playSample(message.index);
      break;
    }
    case "SystemEvent": {
      handleSystemEvent(message.data);
      break;
    }
  }
}

function handleSystemEvent(event) {
  switch (event.type) {
    case "GossipJoined":
      playSample(17);
      break;

    case "GossipLeft":
      break;

    case "GossipNeighborUp":
      console.log(event);
      playSample(18);
      break;

    case "GossipNeighborDown":
      console.log(event);
      playSample(19);
      break;

    case "PeerDiscovered":
      if (peers[event.peer]) {
        break;
      } else {
        peers[event.peer] = new Array();
        playSample(16);
      }
      break;

    case "SyncStarted":
      if (event.topic != null) {
        peers[event.peer].push(playSample(15));
      }
      break;

    case "SyncDone":
      const source = peers[event.peer].shift();
      if (source != undefined) {
        source.stop(0);
        source.disconnect();
      }
      break;

    case "SyncFailed":
      playSample(20);
      break;

    default:
      console.log("Unknown message.data type");
  }
}

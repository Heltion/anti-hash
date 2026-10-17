import init, { worker_main } from './pkg/anti_hash.js';
self.onmessage = ({ data: module }) => {
    self.onmessage = null;
    init({ module_or_path: module }).then(worker_main)
        .catch(error => self.postMessage(['error', String(error)]));
};

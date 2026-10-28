// Capture target IDs before awaits. Responses must not mutate a different device
// merely because it happens to be selected when a response arrives.
(function (root) {
  'use strict';
  function deviceLabel(device, devices) {
    if (device.label) return device.label;
    return devices.filter(other => other.pid === device.pid).length > 1 && device.serial
      ? device.name + ' (' + device.serial.slice(-4) + ')' : device.name;
  }
  function createProfileActions({api, update}) {
    const generations = new Map();
    function begin(target) {
      const id = target.id, generation = (generations.get(id) || 0) + 1;
      generations.set(id, generation);
      const current = () => generations.get(id) === generation;
      return {
        async send(action, name, extra = {}) {
          const response = await api('/api/profile', {id, action, name, ...extra});
          if (!current()) return null;
          if (response.device?.id !== id) throw new Error('Profile response did not match the requested device. Refresh the editor.');
          return response;
        },
        commit(response) {
          if (!current() || !response) return null;
          update(id, response.device);
          return response;
        },
      };
    }
    async function activate(target, name) {
      const operation = begin(target);
      return operation.commit(await operation.send('use', name));
    }
    async function create(target, name) {
      const operation = begin(target);
      const created = await operation.send('new', name);
      if (!created) return null;
      return operation.commit(await operation.send('use', name));
    }
    async function remove(target, name) {
      const other = name === target.active ? target.profiles.find(profile => profile !== name) : null;
      const operation = begin(target);
      if (other && !await operation.send('use', other)) return null;
      return operation.commit(await operation.send('delete', name));
    }
    async function setApps(target, name, apps) {
      const operation = begin(target);
      return operation.commit(await operation.send('apps', name, {apps}));
    }
    return {activate, create, remove, setApps};
  }
  const api = {createProfileActions, deviceLabel};
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  else root.SecbirdProfiles = api;
})(globalThis);

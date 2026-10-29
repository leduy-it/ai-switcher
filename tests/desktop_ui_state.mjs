import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';
const source = await readFile(new URL('../src/desktop-status-state.ts', import.meta.url), 'utf8');
const { desktopStatusState } = await import('data:text/javascript;base64,' + Buffer.from(ts.transpileModule(source, {compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022}}).outputText).toString('base64'));
const base = () => ({
  tools: [{ id:'codex', activeAccountId:'selected', accounts:[{id:'selected',name:'Selected',accountEmail:'person@example.invalid'}]}],
  selectedCodexHome:'/selected', sharedCodexHome:'/shared',
  desktops:[{app:'chatgpt',running:true,installed:true,profileHome:'/selected',sessionHome:'/shared',backendPid:20,usesLocalDaemon:true}],
  desktopSync:{settings:{app:'chatgpt',enabled:true,resumeActiveSessions:true}, confirmedAt:'2026-10-29T00:00:00Z',confirmedEmail:'person@example.invalid',confirmedBackendPid:20,lastAppliedAccountId:'selected',error:null,operation:null},
});
let value=base();assert.equal(desktopStatusState(value).confirmedEmail,'person@example.invalid');
value=base();value.desktops[0].backendPid=21;assert.equal(desktopStatusState(value).confirmedEmail,null,'new process requires new confirmation');
value=base();value.desktops[0].app='codex';assert.equal(desktopStatusState(value).confirmedEmail,null,'wrong brand must not confirm');
value=base();value.desktops[0].usesLocalDaemon=false;value.desktopSync.operation={phase:'waiting',sessions:[]};assert.equal(desktopStatusState(value).canSwitchNow,false,'private transport cannot force recovery');assert.equal(desktopStatusState(value).settingsLocked,false,'safe waiting can be cancelled/configured');
value=base();value.desktopSync.operation={phase:'restoring',sessions:[{phase:'interrupted'}]};assert.equal(desktopStatusState(value).settingsLocked,true);assert.equal(desktopStatusState(value).canSwitchNow,false);
value=base();value.desktopSync.operation={phase:'waiting',sessions:[{phase:'waiting'}]};assert.equal(desktopStatusState(value).canSwitchNow,true);assert.equal(desktopStatusState(value).settingsLocked,false);
value=base();value.desktopSync.operation={phase:'waiting',blockReason:'otherClient',sessions:[]};assert.equal(desktopStatusState(value).canSwitchNow,false);
console.log('7 desktop UI state scenarios passed');

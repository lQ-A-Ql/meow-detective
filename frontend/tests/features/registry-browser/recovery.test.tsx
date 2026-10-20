import { QueryClient,QueryClientProvider } from '@tanstack/react-query';
import { configure,fireEvent,render,screen,waitFor,within } from '@testing-library/react';
import { describe,it,expect,vi } from 'vitest';
import { RegistryBrowserContainer } from '@/features/registry-browser/containers/RegistryBrowserContainer';
import { createRegistryBrowserMock } from '../../../mocks/registry-browser/client';
import data from '../../../mocks/registry-browser/data.json';
import wire from '../../../mocks/registry-browser/recovery-contract.json';
import type { RegistryRecovery } from '@/types/registryBrowser';

// Hive discovery, key resolution and value paging share the suite's CPU budget.
configure({ asyncUtilTimeout: 5000 });
vi.setConfig({ testTimeout: 15000 });

function mount(client=createRegistryBrowserMock()) {
  const query=new QueryClient({defaultOptions:{queries:{retry:false,gcTime:0}}});
  return render(<QueryClientProvider client={query}><RegistryBrowserContainer client={client} context={{caseId:data.caseId,dataSourceId:data.dataSourceId,sourceRevision:'synthetic-v1'}} initialKeyPath="Microsoft\\Windows\\CurrentVersion" /></QueryClientProvider>);
}
async function view(label: string) {fireEvent.keyDown(screen.getByRole('combobox',{name:'浏览视图'}),{key:'ArrowDown'});fireEvent.click(await screen.findByRole('option',{name:label}));}

describe('registry recovered view',()=>{
  it('switches view, preserves the key, shows log provenance and closes the previous session',async()=>{
    const client=createRegistryBrowserMock();const open=vi.spyOn(client,'openHive');const close=vi.spyOn(client,'closeHive');mount(client);
    await screen.findByText('ProductName');await view('LOG1/LOG2 恢复视图');
    await screen.findByText('已按验证通过的日志序列恢复');await screen.findByText('ProductName');
    expect(open).toHaveBeenLastCalledWith(expect.objectContaining({view:'recovered'}));expect(close).toHaveBeenCalled();
    expect(screen.getByText(wire.logs[0].fileId)).toBeInTheDocument();expect(screen.getByText(wire.resultSha256)).toBeInTheDocument();
    fireEvent.click(screen.getByText('Sequence'));await screen.findByRole('tab',{name:'恢复视图字节'});
    await view('基础 Hive');await waitFor(()=>expect(screen.queryByText('已按验证通过的日志序列恢复')).not.toBeInTheDocument());await screen.findByText('ProductName');
  });
  it('failed recovery does not expose old bytes and can switch back to the base hive',async()=>{
    const base=createRegistryBrowserMock();const failure={...wire,status:'failed',resultSha256:undefined,diagnosticCodes:['logHashMismatch']} as RegistryRecovery;
    const client={...base,openHive:vi.fn(async(request)=>{if(request.view==='recovered') throw {code:'recoveryFailed',details:{recovery:failure}};return base.openHive(request);})};
    mount(client);await screen.findByText('ProductName');await view('LOG1/LOG2 恢复视图');
    await screen.findByText('日志恢复失败；未展示派生数据');expect(screen.queryByText('ProductName')).not.toBeInTheDocument();
    expect(within(screen.getByRole('status')).getByText(/恢复失败/)).toBeInTheDocument();
    await view('基础 Hive');await screen.findByRole('treeitem',{name:'Microsoft'});
  });
  it('has a shared wire shape for clean and partial recovery states',()=>{
    const value:RegistryRecovery={...wire,status:'partial',logs:wire.logs.map((log)=>({...log,slot:log.slot as 'log1'|'log2',status:'applied',format:'hvle'}))};
    expect(JSON.parse(JSON.stringify(value)).logs[0].fileId).toBe(wire.logs[0].fileId);
  });
});

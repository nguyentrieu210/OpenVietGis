import React, {memo, useLayoutEffect} from "react";
import type {StudioContribution, StudioHost, StudioPlugin} from "./studio-contract";

export const EMPTY_CONTRIBUTION: StudioContribution = Object.freeze({});
export type ContributionSnapshot = {plugin: StudioPlugin; contribution: StudioContribution};
type Props = {plugin: StudioPlugin; host: StudioHost; publish: (snapshot: ContributionSnapshot) => void; onError: (plugin: StudioPlugin, error: unknown) => void};

// Only this child runs the plugin's hook. Its key can change independently of the
// Core map. Memoization and a stable host prevent publication -> render loops.
const Runtime = memo(function Runtime({plugin,host,publish}:Props){
  const useStudio = plugin.useStudio;
  const contribution = useStudio(host);
  useLayoutEffect(()=>{publish({plugin,contribution});});
  return null;
});
class RuntimeBoundary extends React.Component<Props,{failed:boolean}>{
  state = {failed:false};
  static getDerivedStateFromError(){return {failed:true};}
  componentDidCatch(error:unknown){this.props.onError(this.props.plugin,error);}
  render(){return this.state.failed ? null : <Runtime {...this.props}/>;}
}
export const StudioRuntime = memo(RuntimeBoundary);

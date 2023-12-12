const graphLoadedHandler = window.webkit.messageHandlers.graphLoaded;
const graphErrorHandler = window.webkit.messageHandlers.graphError;

class GraphView {
    constructor() {
        this.div = d3.select("#graph");
        this.graphviz = this.div.graphviz()
            .onerror((error) => {
                graphErrorHandler.postMessage(error);
            })
            .transition(() => {
                return d3.transition().duration(500);
            });

        d3.select(window).on("resize", () => {
            let svg = this.div.selectWithoutDataPropagation("svg");
            svg.attr("width", window.innerWidth).attr("height", window.innerHeight);
        });
    }

    renderGraph(dotSrc, engine) {
        if (dotSrc.length === 0) {
            graphLoadedHandler.postMessage(null);
            return;
        }

        this.graphviz
            .width(window.innerWidth)
            .height(window.innerHeight)
            .fit(true)
            .engine(engine)
            .dot(dotSrc)
            .render(() => {
                graphLoadedHandler.postMessage(null);
            });
    }
}

const graphView = new GraphView();
graphView.renderGraph('', 'dot');

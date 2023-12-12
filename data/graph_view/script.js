class GraphView {
    constructor() {
        this.div = d3.select("#graph");
        this.graphviz = this.div.graphviz()
            .onerror((error) => {
                window.webkit.messageHandlers.graphError.postMessage(error);
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
        this.graphviz
            .width(window.innerWidth)
            .height(window.innerHeight)
            .fit(true)
            .engine(engine)
            .renderDot(dotSrc);
    }
}

const graphView = new GraphView();
graphView.renderGraph('', 'dot');

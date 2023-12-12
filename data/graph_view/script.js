function render(dot, engine) {
    graphviz.engine(engine).renderDot(dot)
}

let graphDiv = d3.select("#graph");

let graphviz = graphDiv.graphviz()
    .onerror((error) => {
        window.webkit.messageHandlers.graphError.postMessage(error);
    })
    .transition(() => {
        return d3.transition().duration(500);
    })
    .on("end", () => {
        resizeSvg();
    });

function resizeSvg() {
    let width = graphDiv.node().parentElement.clientWidth;
    let height = graphDiv.node().parentElement.clientHeight;

    let svg = graphDiv.selectWithoutDataPropagation("svg");
    svg.attr("width", width).attr("height", height);
}

d3.select(window).on("resize", () => {

});
